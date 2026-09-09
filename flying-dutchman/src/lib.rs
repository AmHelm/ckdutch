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
    fn sign(&self, request: SignRequestArgs);
    fn request_app_private_key(&self, request: CKDRequestArgs);
    fn verify_foreign_transaction(&self, request: VerifyForeignTransactionRequestArgs);
}

/// Proxy to the MPC contract with the standard deposit and gas attached.
fn mpc_contract() -> ext_near_mpc::CallNearMpcExt {
    ext_near_mpc::ext(MPC_CONTRACT.parse().unwrap())
        .with_attached_deposit(MPC_CALL_DEPOSIT)
        .with_static_gas(MPC_CALL_GAS)
}

#[near(contract_state)]
#[derive(Default)]
pub struct HelloMpc {}

#[near]
impl HelloMpc {
    /// Requests an ECDSA signature
    pub fn request_signature_ecdsa(&self, message: String) -> Promise {
        let hash: [u8; 32] = Sha256::digest(message.as_bytes()).into();
        let request = SignRequestBuilder::new()
            .with_path("hello-mpc".to_string())
            .with_payload(Payload::Ecdsa(hash.into()))
            .with_domain_id(DomainId(ECDSA_SIGN_DOMAIN_ID))
            .build();
        mpc_contract().sign(request)
    }

    /// Requests an EdDSA (Ed25519) signature.
    pub fn request_signature_eddsa(&self, message: String) -> Promise {
        let payload = Payload::Eddsa(
            message
                .into_bytes()
                .try_into()
                .expect("message must be 32-1232 bytes"),
        );
        let request = SignRequestBuilder::new()
            .with_path("hello-mpc".to_string())
            .with_payload(payload)
            .with_domain_id(DomainId(EDDSA_SIGN_DOMAIN_ID))
            .build();
        mpc_contract().sign(request)
    }

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
        mpc_contract().with_static_gas(gas).request_app_private_key(request)
    }

    /// Asks the MPC network to observe a Bitcoin transaction once it has
    /// `confirmations` confirmations and sign an attestation of the block
    /// hash it landed in. `tx_id` (and `expected_block_hash`, if given) are
    /// hex as block explorers display them. The response is checked in
    /// `on_bitcoin_tx_verified`.
    pub fn verify_bitcoin_tx(
        &self,
        tx_id: String,
        confirmations: u64,
        expected_block_hash: Option<String>,
    ) -> Promise {
        let (_verifier, request) = bitcoin_request(&tx_id, confirmations, &expected_block_hash);
        mpc_contract().verify_foreign_transaction(request).then(
            Self::ext(env::current_account_id())
                .with_static_gas(CALLBACK_GAS)
                .on_bitcoin_tx_verified(tx_id, confirmations, expected_block_hash),
        )
    }

    /// Verifies the MPC response: the signed payload hash must match our
    /// request + expectations, and the signature must check out against the
    /// foreign-tx domain's public key.
    #[private]
    pub fn on_bitcoin_tx_verified(
        &self,
        tx_id: String,
        confirmations: u64,
        expected_block_hash: Option<String>,
        #[callback_unwrap] response: VerifyForeignTransactionResponse,
    ) -> VerifyForeignTransactionResponse {
        let (verifier, _request) = bitcoin_request(&tx_id, confirmations, &expected_block_hash);
        let public_key: PublicKey = FOREIGN_TX_PUBLIC_KEY.parse().expect("valid public key");
        if let Err(err) = verifier.verify_signature(&response, &public_key) {
            env::panic_str(match err {
                VerifyForeignChainError::FailedToComputeMsgHash => "failed to compute msg hash",
                VerifyForeignChainError::IncorrectPayloadSigned { .. } => {
                    "signed payload does not match request and expectations"
                }
                VerifyForeignChainError::UnexpectedSignatureScheme => {
                    "unexpected signature scheme"
                }
                VerifyForeignChainError::SignatureVerificationFailed => {
                    "signature verification failed"
                }
            });
        }
        env::log_str(&format!("verified: bitcoin tx {tx_id} is on-chain"));
        response
    }
}

/// Builds the foreign-tx request plus a verifier for its response. The
/// builder binds `expected_payload_hash` so the MPC network can only answer
/// with the values we expect.
fn bitcoin_request(
    tx_id: &str,
    confirmations: u64,
    expected_block_hash: &Option<String>,
) -> (
    ForeignChainSignatureVerifier,
    VerifyForeignTransactionRequestArgs,
) {
    let builder = ForeignChainRequestBuilder::new_bitcoin()
        .with_tx_id(decode_hash(tx_id))
        .with_block_confirmations(confirmations);
    let builder = match expected_block_hash {
        Some(block_hash) => builder.with_expected_block_hash(decode_hash(block_hash)),
        None => builder,
    };
    builder
        .with_domain_id(DomainId(FOREIGN_TX_DOMAIN_ID))
        .build()
        .expect("serializing the expected payload cannot fail")
}

fn decode_hash(hex_str: &str) -> [u8; 32] {
    hex::decode(hex_str)
        .expect("must be hex")
        .try_into()
        .expect("must be 32 bytes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::serde_json;

    #[test]
    fn wire_format() {
        let request = SignRequestBuilder::new()
            .with_path("hello-mpc".to_string())
            .with_payload(Payload::Ecdsa([0xab; 32].into()))
            .with_domain_id(DomainId(ECDSA_SIGN_DOMAIN_ID))
            .build();
        println!("sign: {}", serde_json::json!({ "request": request }));

        let request = SignRequestBuilder::new()
            .with_path("hello-mpc".to_string())
            .with_payload(Payload::Eddsa(vec![0xab; 32].try_into().unwrap()))
            .with_domain_id(DomainId(EDDSA_SIGN_DOMAIN_ID))
            .build();
        println!("sign eddsa: {}", serde_json::json!({ "request": request }));

        let (_verifier, request) = bitcoin_request(&"cd".repeat(32), 3, &None);
        println!("foreign_tx: {}", serde_json::json!({ "request": request }));

        let (_verifier, request) =
            bitcoin_request(&"cd".repeat(32), 3, &Some("ef".repeat(32)));
        println!(
            "foreign_tx with expectations: {}",
            serde_json::json!({ "request": request })
        );
    }
}
