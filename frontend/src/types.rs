use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

pub const RPC_URL: &str = "https://rpc.testnet.fastnear.com";
pub const GLOBAL_CONTRACT: &str = "flying-dman.testnet";
pub const MPC_CONTRACT: &str = "v1.signer-prod.testnet";
pub const DOMAIN_ID: u64 = 2;
pub const DERIVATION_PATH: &str = "d";
pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

pub fn account_id(value: &str) -> Result<String> {
    let value = value.trim();
    value.parse::<near_account_id::AccountId>()?;
    ensure!(value.ends_with(".testnet"), "Use a named .testnet account");
    Ok(value.to_string())
}

#[derive(Clone, Debug, Deserialize)]
pub struct SwitchStatus {
    pub owner: String,
    pub challenge_deadline_ms: Option<u64>,
    pub challenge_delay_ms: u64,
    pub friends: Vec<String>,
    pub now_ms: u64,
    pub key_is_public: bool,
    pub version: String,
}

/// Public envelope. All metadata is authenticated as AES-GCM associated data.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub version: u32,
    pub network: String,
    pub contract: String,
    pub mpc_contract: String,
    pub domain_id: u64,
    pub derivation_path: String,
    pub mpc_public_key: String,
    pub algorithm: String,
    pub filename: String,
    pub nonce: String,
    pub ciphertext: String,
}

impl Envelope {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && self.network == "testnet",
            "Unsupported capsule version or network"
        );
        account_id(&self.contract)?;
        ensure!(
            self.mpc_contract == MPC_CONTRACT
                && self.domain_id == DOMAIN_ID
                && self.derivation_path == DERIVATION_PATH,
            "Unsupported CKD parameters"
        );
        ensure!(
            self.algorithm == "AES-256-GCM/HKDF-SHA256",
            "Unsupported encryption algorithm"
        );
        ensure!(self.filename.len() <= 255, "Filename is too long");
        ensure!(
            self.ciphertext.len() <= (MAX_FILE_BYTES + 16).div_ceil(3) * 4,
            "Capsule exceeds 10 MiB"
        );
        Ok(())
    }

    pub fn aad(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(
            self.version,
            &self.network,
            &self.contract,
            &self.mpc_contract,
            self.domain_id,
            &self.derivation_path,
            &self.mpc_public_key,
            &self.algorithm,
            &self.filename,
        ))?)
    }
}
