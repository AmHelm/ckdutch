//! Browser bindings for the existing CKDutch capsule format and MPC protocol.
mod crypto;
mod types;

use wasm_bindgen::prelude::*;
use zeroize::Zeroizing;

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// An ephemeral MPC request. Release it with `.free()` after deriving the key.
#[wasm_bindgen]
pub struct CkdRequest(crypto::Ephemeral);

#[wasm_bindgen]
impl CkdRequest {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(crypto::Ephemeral::generate())
    }

    #[wasm_bindgen(js_name = publicKey)]
    pub fn public_key(&self) -> String {
        self.0.public_key()
    }

    pub fn derive(
        &self,
        response_json: &str,
        contract: &str,
        mpc_key: &str,
    ) -> Result<CapsuleKey, JsValue> {
        let response = serde_json::from_str(response_json).map_err(js_error)?;
        let key = self
            .0
            .decrypt(&response, contract, mpc_key)
            .map_err(js_error)?;
        Ok(CapsuleKey(key))
    }
}

impl Default for CkdRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// A verified derived key. Its bytes remain in WASM; `.free()` zeroizes them.
#[wasm_bindgen]
pub struct CapsuleKey(Zeroizing<[u8; 32]>);

#[wasm_bindgen]
impl CapsuleKey {
    pub fn encrypt(
        &self,
        contract: &str,
        mpc_key: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<String, JsValue> {
        let envelope =
            crypto::encrypt(&self.0, contract, mpc_key, filename, data).map_err(js_error)?;
        serde_json::to_string(&envelope).map_err(js_error)
    }

    pub fn decrypt(&self, capsule_json: &str) -> Result<Vec<u8>, JsValue> {
        let envelope = serde_json::from_str(capsule_json).map_err(js_error)?;
        let plaintext = crypto::decrypt(&self.0, &envelope).map_err(js_error)?;
        Ok(plaintext.to_vec())
    }
}

/// Validate before showing capsule metadata or requesting its MPC key.
#[wasm_bindgen(js_name = validateCapsule)]
pub fn validate_capsule(capsule_json: &str) -> Result<String, JsValue> {
    let envelope: types::Envelope = serde_json::from_str(capsule_json).map_err(js_error)?;
    envelope.validate().map_err(js_error)?;
    serde_json::to_string(&envelope).map_err(js_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_request_derives_verified_key_and_opens_capsule() {
        use bls12_381::{
            hash_to_curve::{ExpandMsgXmd, HashToCurve},
            G1Affine, G1Projective, G2Affine, G2Projective, Scalar,
        };
        use sha3::{Digest, Sha3_256};

        let request = CkdRequest::new();
        let mpc_secret = Scalar::from(7u64);
        let blind = Scalar::from(5u64);
        let mpc_bytes = G2Affine::from(G2Projective::generator() * mpc_secret).to_compressed();
        let mpc_key = format!("bls12381g2:{}", bs58::encode(mpc_bytes).into_string());
        let app_id = Sha3_256::digest(b"near-mpc v0.1.0 app_id derivation:owner.testnet,d");
        let hash = <G1Projective as HashToCurve<ExpandMsgXmd<sha2_09::Sha256>>>::hash_to_curve(
            [mpc_bytes.as_slice(), app_id.as_slice()].concat(),
            b"NEAR BLS12381G1_XMD:SHA-256_SSWU_RO_",
        );
        let app_bytes = crypto::decode_point(&request.public_key(), "bls12381g1:").unwrap();
        let app = G1Affine::from_compressed(&app_bytes).unwrap();
        let encode = |p: G1Projective| {
            format!(
                "bls12381g1:{}",
                bs58::encode(G1Affine::from(p).to_compressed()).into_string()
            )
        };
        let response = serde_json::json!({
            "big_y": encode(G1Projective::generator() * blind),
            "big_c": encode(hash * mpc_secret + app * blind),
        });
        let key = request
            .derive(&response.to_string(), "owner.testnet", &mpc_key)
            .unwrap();
        let capsule = key
            .encrypt("owner.testnet", &mpc_key, "note.txt", b"kept private")
            .unwrap();
        assert_eq!(key.decrypt(&capsule).unwrap(), b"kept private");
    }

    #[test]
    fn exported_capsule_boundary_preserves_unicode_metadata_and_binary_data() {
        let key = CapsuleKey(Zeroizing::new([42; 32]));
        let data = [0, 1, 127, 128, 255];
        let capsule = key
            .encrypt("owner.testnet", "test-key", "pro tebe 🗝️.txt", &data)
            .unwrap();
        let validated = validate_capsule(&capsule).unwrap();
        assert_eq!(validated, capsule);
        assert_eq!(key.decrypt(&validated).unwrap(), data);
    }
}
