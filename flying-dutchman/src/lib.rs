//! Example contract calling public methods on the MPC contract

use near_mpc_sdk::foreign_chain::{
    ForeignChainRequestBuilder, ForeignChainSignatureVerifier, VerifyForeignChainError,
    VerifyForeignTransactionRequestArgs, VerifyForeignTransactionResponse,
};
use near_mpc_sdk::near_mpc_contract_interface::types::{
    CKDAppPublicKey, CKDRequestArgs, DomainId, Payload, PublicKey,
};
use near_mpc_sdk::sign::{SignRequestArgs, SignRequestBuilder};
use near_sdk::{env, ext_contract, near, Gas, NearToken, Promise};
use sha2::{Digest, Sha256};

/// The MPC contract on testnet (mainnet: `v1.signer`).
const MPC_CONTRACT: &str = "v1.signer-prod.testnet";

/// Domain ids on the testnet deployment; query `state()` on the MPC contract
/// to see these domains, their schemes and public keys.
const ECDSA_SIGN_DOMAIN_ID: u64 = 0;
const EDDSA_SIGN_DOMAIN_ID: u64 = 1;
const CKD_DOMAIN_ID: u64 = 2;
const FOREIGN_TX_DOMAIN_ID: u64 = 3;

/// Public key of the foreign-tx domain (id 3) on testnet, from `state()`.
/// Foreign-tx responses are signed with the domain's root key.
const FOREIGN_TX_PUBLIC_KEY: &str = "secp256k1:2KGCoy2pZt7n85QfJnQzCT1eHySuNpquUfNc8ySpRbr15F5kqU7agJYjfo5RkrzNd4tZimDFv2wAZri7RRZ32qj1";

/// The MPC contract requires at least 10 Tgas attached per request.
const MPC_CALL_GAS: Gas = Gas::from_tgas(30);
/// Publicly verifiable CKD requests run a pairing check on-chain and need
/// significantly more gas.
const CKD_PV_CALL_GAS: Gas = Gas::from_tgas(100);
/// Every MPC request requires a deposit of at least 1 yoctoNEAR.
const MPC_CALL_DEPOSIT: NearToken = NearToken::from_yoctonear(1);
/// Gas for the response-verification callback.
const CALLBACK_GAS: Gas = Gas::from_tgas(10);

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
pub struct FlyingDutchman {}

#[near]
impl FlyingDutchman {
    /// Requests a private key derived from this contract's account id and
    /// `derivation_path`, returned encrypted to `app_public_key` (e.g.
    /// `"bls12381g1:<base58>"`). Use the `ckd-example-cli` in the mpc repo to
    /// generate the app keypair and decrypt the response.
    pub fn request_confidential_key(
        &self,
        derivation_path: String,
        app_public_key: CKDAppPublicKey,
    ) -> Promise {
        let gas = match &app_public_key {
            CKDAppPublicKey::AppPublicKey(_) => MPC_CALL_GAS,
            CKDAppPublicKey::AppPublicKeyPV(_) => CKD_PV_CALL_GAS,
        };
        let request = CKDRequestArgs {
            derivation_path,
            app_public_key,
            domain_id: DomainId(CKD_DOMAIN_ID),
        };
        mpc_contract()
            .with_static_gas(gas)
            .request_app_private_key(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::serde_json;

    #[test]
    fn placeholder() {
        assert!(false)
    }
}
