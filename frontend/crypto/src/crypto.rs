//! WASM-compatible implementation of NEAR's ckd-example-cli protocol.
//! The BLS pairing is verified before applying the CLI's purpose-tagged HKDF.
use crate::types::*;
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use bls12_381::{
    hash_to_curve::{ExpandMsgXmd, HashToCurve},
    pairing, G1Affine, G1Projective, G2Affine, Scalar,
};
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sha3::Sha3_256;
use zeroize::Zeroizing;

const DST: &[u8] = b"NEAR BLS12381G1_XMD:SHA-256_SSWU_RO_";

pub struct Ephemeral(Zeroizing<[u8; 32]>);

#[derive(Deserialize)]
pub struct CkdResponse {
    pub big_y: String,
    pub big_c: String,
}

pub fn decode_point<const N: usize>(value: &str, prefix: &str) -> Result<[u8; N]> {
    let raw = value
        .strip_prefix(prefix)
        .context("Unexpected public key type")?;
    bs58::decode(raw)
        .into_vec()?
        .try_into()
        .map_err(|_| anyhow!("Invalid public key length"))
}

fn g1(value: &str) -> Result<G1Projective> {
    let bytes = decode_point(value, "bls12381g1:")?;
    let p = Option::<G1Affine>::from(G1Affine::from_compressed(&bytes))
        .context("Invalid BLS G1 point")?;
    ensure!(!bool::from(p.is_identity()), "Identity point rejected");
    Ok(p.into())
}

impl Ephemeral {
    pub fn generate() -> Self {
        loop {
            let mut wide = Zeroizing::new([0u8; 64]);
            OsRng.fill_bytes(wide.as_mut());
            let scalar = Scalar::from_bytes_wide(&wide);
            if scalar != Scalar::zero() {
                return Self(Zeroizing::new(scalar.to_bytes()));
            }
        }
    }

    fn scalar(&self) -> Scalar {
        Option::<Scalar>::from(Scalar::from_bytes(&self.0)).expect("Generated canonical scalar")
    }

    pub fn public_key(&self) -> String {
        let point = G1Affine::from(G1Projective::generator() * self.scalar()).to_compressed();
        format!("bls12381g1:{}", bs58::encode(point).into_string())
    }

    pub fn decrypt(
        &self,
        response: &CkdResponse,
        contract: &str,
        mpc_key: &str,
    ) -> Result<Zeroizing<[u8; 32]>> {
        let app_id = Sha3_256::digest(format!(
            "near-mpc v0.1.0 app_id derivation:{contract},{DERIVATION_PATH}"
        ));
        self.decrypt_for_app(response, &app_id, mpc_key)
    }

    fn decrypt_for_app(
        &self,
        response: &CkdResponse,
        app_id: &[u8],
        mpc_key: &str,
    ) -> Result<Zeroizing<[u8; 32]>> {
        let pk_bytes = decode_point(mpc_key, "bls12381g2:")?;
        let pk = Option::<G2Affine>::from(G2Affine::from_compressed(&pk_bytes))
            .context("Invalid MPC public key")?;
        ensure!(!bool::from(pk.is_identity()), "Identity MPC key rejected");
        let secret = G1Affine::from(g1(&response.big_c)? - g1(&response.big_y)? * self.scalar());
        ensure!(
            !bool::from(secret.is_identity()),
            "Identity CKD secret rejected"
        );
        let input = [pk_bytes.as_slice(), app_id].concat();
        let hash = <G1Projective as HashToCurve<ExpandMsgXmd<sha2_09::Sha256>>>::hash_to_curve(
            &input, DST,
        );
        ensure!(
            pairing(&G1Affine::from(hash), &pk) == pairing(&secret, &G2Affine::generator()),
            "CKD verification failed: response, account, or MPC public key does not match"
        );
        let compressed = Zeroizing::new(secret.to_compressed());
        let mut key = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(Some(b"near-mpc-ckd-hkdf-v1"), compressed.as_ref())
            .expand(b"near-mpc-ckd-strong-key-v1", key.as_mut())
            .map_err(|_| anyhow!("HKDF failed"))?;
        Ok(key)
    }
}

pub fn encrypt(
    key: &[u8; 32],
    contract: &str,
    mpc_key: &str,
    filename: &str,
    data: &[u8],
) -> Result<Envelope> {
    ensure!(data.len() <= MAX_FILE_BYTES, "Maximum file size is 10 MiB");
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let mut envelope = Envelope {
        version: 1,
        network: "testnet".into(),
        contract: account_id(contract)?,
        mpc_contract: MPC_CONTRACT.into(),
        domain_id: DOMAIN_ID,
        derivation_path: DERIVATION_PATH.into(),
        mpc_public_key: mpc_key.into(),
        algorithm: "AES-256-GCM/HKDF-SHA256".into(),
        filename: filename.into(),
        nonce: STANDARD.encode(nonce),
        ciphertext: String::new(),
    };
    envelope.validate()?;
    let encrypted = Aes256Gcm::new(key.into())
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: data,
                aad: &envelope.aad()?,
            },
        )
        .map_err(|_| anyhow!("Encryption failed"))?;
    envelope.ciphertext = STANDARD.encode(encrypted);
    Ok(envelope)
}

pub fn decrypt(key: &[u8; 32], envelope: &Envelope) -> Result<Zeroizing<Vec<u8>>> {
    envelope.validate()?;
    let nonce: [u8; 12] = STANDARD
        .decode(&envelope.nonce)?
        .try_into()
        .map_err(|_| anyhow!("Invalid nonce length"))?;
    let data = STANDARD.decode(&envelope.ciphertext)?;
    Aes256Gcm::new(key.into())
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &data,
                aad: &envelope.aad()?,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| {
            anyhow!("Authentication failed: the capsule was changed or the key does not match")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_tamper_detection() {
        let key = [42u8; 32];
        let capsule = encrypt(
            &key,
            "owner.testnet",
            "test-key",
            "note.txt",
            b"for my family",
        )
        .unwrap();
        assert_eq!(
            decrypt(&key, &capsule).unwrap().as_slice(),
            b"for my family"
        );
        assert!(decrypt(&[41; 32], &capsule).is_err());
        let mut changed = capsule.clone();
        changed.contract = "other.testnet".into();
        assert!(decrypt(&key, &changed).is_err());
        changed = capsule.clone();
        changed.filename = "changed.txt".into();
        assert!(decrypt(&key, &changed).is_err());
        changed = capsule.clone();
        changed.nonce = STANDARD.encode([1; 11]);
        assert!(decrypt(&key, &changed).is_err());
        assert_ne!(
            capsule.nonce,
            encrypt(
                &key,
                "owner.testnet",
                "test-key",
                "note.txt",
                b"for my family"
            )
            .unwrap()
            .nonce
        );
    }

    #[test]
    fn upstream_bls_vector_and_wrong_account() {
        // near/mpc d45bd9b, threshold-signatures/tests/vectors/ckd_test_vectors.json, vector 0.
        let point = |prefix: &str, hex: &str| {
            format!(
                "{prefix}:{}",
                bs58::encode(hex::decode(hex).unwrap()).into_string()
            )
        };
        let mut scalar: [u8; 32] =
            hex::decode("03eb678530e40762c0e94bad392207830268e73379c460f09ae7a621c9120bfd")
                .unwrap()
                .try_into()
                .unwrap();
        scalar.reverse();
        let ephemeral = Ephemeral(Zeroizing::new(scalar));
        assert_eq!(ephemeral.public_key(), point("bls12381g1", "863b7e3f9a763be4adbe64f9b52ba862c615339dfbaf0fb9bbf5e971c21a7d8eed17b1b16ff88fde7bac8aa6eee77360"));
        let response = CkdResponse {
            big_c: point("bls12381g1", "8967b5a379290b42c371a312d5c824f47c4c1b0ab09cc09da60fe4155448e6fbd77f134c5f66ca7545e4457baffd269e"),
            big_y: point("bls12381g1", "8c17ae06c27665ea865ab90226fa869c442238451f3cfc8e15ed49e8843c0a69a9949faecd0291c2849893b386ae2d12"),
        };
        let pk = point("bls12381g2", "acd353f38a555fa11a929f391a19240d6b24d4f721db16db3fad73a3d29ce7220142cd69b8665d510af239b91f8c98140f651de3bd6be6061f03c9b21438f444f965fd6e1d0afdc44b66f16cd217fb02ad2f0409b51b98af54551781a48a6707");
        let app_id =
            hex::decode("c7dba29ba7ae16fd28796c2f508bc11d571c38341ce55d243b3c72cf02e0a2fd")
                .unwrap();
        let key = ephemeral.decrypt_for_app(&response, &app_id, &pk).unwrap();
        let sig = hex::decode("b7a83185c068fe7098766c005cef35a794e37311cdf989e183e0364262953e58eb225cd31e0d8c2c166aa3276e668362").unwrap();
        let mut expected = [0u8; 32];
        Hkdf::<Sha256>::new(Some(b"near-mpc-ckd-hkdf-v1"), &sig)
            .expand(b"near-mpc-ckd-strong-key-v1", &mut expected)
            .unwrap();
        assert_eq!(*key, expected);
        assert!(ephemeral.decrypt(&response, "wrong.testnet", &pk).is_err());
        assert!(Ephemeral::generate()
            .decrypt_for_app(&response, &app_id, &pk)
            .is_err());
    }
}
