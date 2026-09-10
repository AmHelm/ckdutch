//! Example contract calling public methods on the MPC contract

use near_mpc_sdk::near_mpc_contract_interface::types::{CKDAppPublicKey, CKDRequestArgs, DomainId};
use near_sdk::{env, ext_contract, near, AccountId, Gas, NearToken, Promise};

/// The MPC contract on testnet (mainnet: `v1.signer`).
const MPC_CONTRACT: &str = "v1.signer-prod.testnet";

/// Domain ids on the testnet deployment; query `state()` on the MPC contract
/// to see these domains, their schemes and public keys.
const CKD_DOMAIN_ID: u64 = 2;

/// The MPC contract requires at least 10 Tgas attached per request.
const MPC_CALL_GAS: Gas = Gas::from_tgas(30);
/// Publicly verifiable CKD requests run a pairing check on-chain and need
/// significantly more gas.
const CKD_PV_CALL_GAS: Gas = Gas::from_tgas(100);
/// Every MPC request requires a deposit of at least 1 yoctoNEAR.
const MPC_CALL_DEPOSIT: NearToken = NearToken::from_yoctonear(1);

/// The MPC contract methods we call; `#[ext_contract]` generates the typed
/// `ext_near_mpc` proxy for making these cross-contract calls.
#[ext_contract(ext_near_mpc)]
#[allow(dead_code)] // only the generated `ext_near_mpc` proxy is used
trait CallNearMpc {
    fn request_app_private_key(&self, request: CKDRequestArgs);
}

/// Proxy to the MPC contract with the standard deposit and gas attached.
fn mpc_contract() -> ext_near_mpc::CallNearMpcExt {
    ext_near_mpc::ext(MPC_CONTRACT.parse().unwrap())
        .with_attached_deposit(MPC_CALL_DEPOSIT)
        .with_static_gas(MPC_CALL_GAS)
}

#[near(contract_state)]
#[derive(Default)]
pub struct FlyingDutchman {
    challenge: Option<u64>, // Release deadline in milliseconds since Unix epoch.
    friends: Vec<AccountId>,
    challenge_delay_ms: u64,
}

#[near(serializers = [json])]
pub struct SwitchStatus {
    pub owner: AccountId,
    pub challenge_deadline_ms: Option<u64>,
    pub challenge_delay_ms: u64,
    pub friends: Vec<AccountId>,
    pub now_ms: u64,
    pub key_is_public: bool,
    pub version: String,
}

#[near]
impl FlyingDutchman {
    #[init]
    pub fn init(challenge_delay_ms: u64, friends: Vec<AccountId>) -> Self {
        assert!(challenge_delay_ms > 0, "Challenge delay must be positive");
        FlyingDutchman {
            challenge: None,
            friends,
            challenge_delay_ms,
        }
    }

    /// Start the challenge
    pub fn claim_owner_is_dead(&mut self) {
        if self.challenge.is_some() {
            env::panic_str("Challenge already active");
        } else {
            self.challenge = Some(
                env::block_timestamp_ms()
                    .checked_add(self.challenge_delay_ms)
                    .expect("Challenge deadline overflow"),
            );
        }
    }

    /// Say that the person is still alive
    pub fn claim_owner_is_alive(&mut self) {
        let caller_account = env::predecessor_account_id();

        if caller_account == env::current_account_id() || self.friends.contains(&caller_account) {
            self.challenge = None;
        } else {
            env::panic_str(&format!("{caller_account} not authorized"));
        }
    }

    /// Requests a private key derived from this contract's account id and
    /// the fixed derivation path `d`, returned encrypted to `app_public_key` (e.g.
    /// `"bls12381g1:<base58>"`). Use the `ckd-example-cli` in the mpc repo to
    /// generate the app keypair and decrypt the response.
    pub fn request_confidential_key(&self, app_public_key: CKDAppPublicKey) -> Promise {
        let is_owner = env::predecessor_account_id() == env::current_account_id();
        let is_challenge_expired = match self.challenge.as_ref() {
            Some(challenge_time) => env::block_timestamp_ms() > *challenge_time,
            None => false,
        };

        if !is_owner && !is_challenge_expired {
            env::panic_str("Only the owner may request a key before the challenge expires");
        }

        let gas = match &app_public_key {
            CKDAppPublicKey::AppPublicKey(_) => MPC_CALL_GAS,
            CKDAppPublicKey::AppPublicKeyPV(_) => CKD_PV_CALL_GAS,
        };
        let request = CKDRequestArgs {
            derivation_path: "d".to_string(),
            app_public_key,
            domain_id: DomainId(CKD_DOMAIN_ID),
        };
        mpc_contract()
            .with_static_gas(gas)
            .request_app_private_key(request)
    }

    pub fn get_status(&self) -> SwitchStatus {
        let now_ms = env::block_timestamp_ms();
        SwitchStatus {
            owner: env::current_account_id(),
            challenge_deadline_ms: self.challenge,
            challenge_delay_ms: self.challenge_delay_ms,
            friends: self.friends.clone(),
            now_ms,
            key_is_public: self.challenge.is_some_and(|deadline| now_ms >= deadline),
            version: "1".to_string(),
        }
    }

    pub fn add_friend(&mut self, friend: AccountId) {
        let caller_account = env::predecessor_account_id();

        if caller_account != env::current_account_id() {
            env::panic_str(&format!("{caller_account} not authorized"));
        } else {
            self.friends.push(friend);
        }
    }

    pub fn remove_friend(&mut self, friend: AccountId) {
        let caller_account = env::predecessor_account_id();

        if caller_account != env::current_account_id() {
            env::panic_str(&format!("{caller_account} not authorized"));
        } else {
            self.friends.retain(|f| friend != *f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::{test_utils::VMContextBuilder, testing_env};

    fn context(caller: &str, time_ms: u64) {
        testing_env!(VMContextBuilder::new()
            .current_account_id("owner.testnet".parse().unwrap())
            .predecessor_account_id(caller.parse().unwrap())
            .block_timestamp(time_ms * 1_000_000)
            .account_balance(NearToken::from_near(10))
            .build());
    }

    fn public_key() -> CKDAppPublicKey {
        use near_mpc_sdk::near_mpc_contract_interface::types::Bls12381G1PublicKey;
        let bytes = hex::decode("97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb").unwrap();
        CKDAppPublicKey::AppPublicKey(Bls12381G1PublicKey(bytes.try_into().unwrap()))
    }

    #[test]
    fn deadline_and_cancellation() {
        context("visitor.testnet", 1_800_000_000_000);
        let mut contract = FlyingDutchman::init(60_000, vec!["friend.testnet".parse().unwrap()]);
        contract.claim_owner_is_dead();
        let deadline = contract.get_status().challenge_deadline_ms.unwrap();
        assert_eq!(deadline, 1_800_000_060_000);
        context("visitor.testnet", deadline - 1);
        assert!(!contract.get_status().key_is_public);
        context("visitor.testnet", deadline);
        assert!(contract.get_status().key_is_public);
        context("friend.testnet", deadline - 1);
        contract.claim_owner_is_alive();
        assert_eq!(contract.get_status().challenge_deadline_ms, None);
    }

    #[test]
    #[should_panic(expected = "Only the owner")]
    fn public_cannot_request_early_even_at_realistic_timestamps() {
        context("visitor.testnet", 1_800_000_000_000);
        let mut contract = FlyingDutchman::init(60_000, vec![]);
        contract.claim_owner_is_dead();
        context("visitor.testnet", 1_800_000_000_001);
        contract.request_confidential_key(public_key()).detach();
    }

    #[test]
    fn owner_can_request_without_challenge_and_public_at_deadline() {
        context("owner.testnet", 1_800_000_000_000);
        let mut contract = FlyingDutchman::init(60_000, vec![]);
        contract.request_confidential_key(public_key()).detach();
        contract.claim_owner_is_dead();
        context("visitor.testnet", 1_800_000_060_000);
        contract.request_confidential_key(public_key()).detach();
    }

    #[test]
    #[should_panic(expected = "not authorized")]
    fn stranger_cannot_cancel() {
        context("visitor.testnet", 1_000);
        let mut contract = FlyingDutchman::init(60_000, vec![]);
        contract.claim_owner_is_dead();
        contract.claim_owner_is_alive();
    }

    #[test]
    #[should_panic(expected = "already active")]
    fn challenge_cannot_be_extended_by_a_stranger() {
        context("visitor.testnet", 1_000);
        let mut contract = FlyingDutchman::init(60_000, vec![]);
        contract.claim_owner_is_dead();
        contract.claim_owner_is_dead();
    }
}
