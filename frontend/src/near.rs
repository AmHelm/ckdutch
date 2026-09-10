//! Minimal NEAR V0 transaction encoding, shared by browser and native smoke test.
//! Wire layout: nearcore core/primitives/src/{transaction.rs,action/mod.rs}.
use crate::{
    crypto::{CkdResponse, Ephemeral},
    types::*,
};
use anyhow::{anyhow, bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use borsh::BorshSerialize;
use ed25519_dalek::{Signer as _, SigningKey};
use rand::rngs::OsRng;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct Signer {
    pub account: String,
    key: SigningKey,
}

impl Signer {
    pub fn import(account: &str, secret: &str) -> Result<Self> {
        let account = account_id(account)?;
        let secret = Zeroizing::new(
            bs58::decode(
                secret
                    .trim()
                    .strip_prefix("ed25519:")
                    .context("Expected an ed25519: private key")?,
            )
            .into_vec()?,
        );
        ensure!(secret.len() == 64, "NEAR private keys contain 64 bytes");
        let bytes: &[u8; 64] = secret.as_slice().try_into()?;
        let key = SigningKey::from_keypair_bytes(bytes).context("Invalid NEAR keypair")?;
        Ok(Self { account, key })
    }

    pub fn generate(account: &str) -> Result<Self> {
        Ok(Self {
            account: account_id(account)?,
            key: SigningKey::generate(&mut OsRng),
        })
    }

    pub fn public_key(&self) -> String {
        format!(
            "ed25519:{}",
            bs58::encode(self.key.verifying_key().to_bytes()).into_string()
        )
    }

    pub fn credentials(&self) -> Result<Zeroizing<String>> {
        let secret = Zeroizing::new(format!(
            "ed25519:{}",
            bs58::encode(self.key.to_keypair_bytes()).into_string()
        ));
        Ok(Zeroizing::new(serde_json::to_string_pretty(&json!({
            "account_id": self.account, "public_key": self.public_key(), "private_key": *secret,
        }))?))
    }

    pub fn add_key_action(&self) -> Action {
        Action::AddKey {
            public_key: PublicKey::Ed25519(self.key.verifying_key().to_bytes()),
            nonce: 0,
            permission: FullAccess::FullAccess,
        }
    }

    fn sign(
        &self,
        receiver: &str,
        nonce: u64,
        block_hash: [u8; 32],
        actions: Vec<Action>,
    ) -> Result<(String, String)> {
        let tx = Transaction {
            signer_id: self.account.clone(),
            public_key: PublicKey::Ed25519(self.key.verifying_key().to_bytes()),
            nonce,
            receiver_id: account_id(receiver)?,
            block_hash,
            actions,
        };
        let mut bytes = borsh::to_vec(&tx)?;
        let hash = Sha256::digest(&bytes);
        let signature = self.key.sign(&hash);
        bytes.push(0); // Signature::ED25519
        bytes.extend_from_slice(&signature.to_bytes());
        Ok((STANDARD.encode(bytes), bs58::encode(hash).into_string()))
    }
}

#[derive(BorshSerialize, Clone)]
pub enum PublicKey {
    Ed25519([u8; 32]),
}

#[derive(BorshSerialize, Clone)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum FullAccess {
    FullAccess = 1,
}

#[derive(BorshSerialize, Clone)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum GlobalId {
    AccountId(String) = 1,
}

#[derive(BorshSerialize, Clone)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum GlobalDeployMode {
    AccountId = 1,
}

#[derive(BorshSerialize, Clone)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum Action {
    CreateAccount = 0,
    DeployContract {
        code: Vec<u8>,
    } = 1,
    FunctionCall {
        method_name: String,
        args: Vec<u8>,
        gas: u64,
        deposit: u128,
    } = 2,
    Transfer {
        deposit: u128,
    } = 3,
    AddKey {
        public_key: PublicKey,
        nonce: u64,
        permission: FullAccess,
    } = 5,
    DeployGlobalContract {
        code: Vec<u8>,
        deploy_mode: GlobalDeployMode,
    } = 9,
    UseGlobalContract {
        contract_identifier: GlobalId,
    } = 10,
}

#[derive(BorshSerialize)]
struct Transaction {
    signer_id: String,
    public_key: PublicKey,
    nonce: u64,
    receiver_id: String,
    block_hash: [u8; 32],
    actions: Vec<Action>,
}

pub fn call_action(method: &str, args: Value) -> Result<Action> {
    Ok(Action::FunctionCall {
        method_name: method.into(),
        args: serde_json::to_vec(&args)?,
        gas: 150_000_000_000_000,
        deposit: 0,
    })
}

pub struct Outcome {
    pub hash: String,
    pub value: Value,
}

#[derive(Clone)]
pub struct Rpc {
    pub url: String,
}

impl Default for Rpc {
    fn default() -> Self {
        Self {
            url: RPC_URL.into(),
        }
    }
}

/// Public NEAR endpoints throttle short bursts and recover within seconds, so
/// these statuses are worth waiting out rather than reporting.
fn worth_retrying(status: u16) -> bool {
    matches!(status, 429 | 502 | 503 | 504)
}

impl Rpc {
    /// One round trip. `Ok(None)` means the endpoint asked us to come back.
    async fn attempt(&self, body: &Value) -> Result<Option<Value>> {
        #[cfg(target_arch = "wasm32")]
        {
            let controller = web_sys::AbortController::new()
                .map_err(|_| anyhow!("Cannot initialize RPC request"))?;
            let signal = controller.signal();
            let _timeout = gloo_timers::callback::Timeout::new(30_000, move || controller.abort());
            let response = gloo_net::http::Request::post(&self.url)
                .abort_signal(Some(&signal))
                .json(body)?
                .send()
                .await
                .map_err(|e| anyhow!("RPC connection failed: {e}"))?;
            if worth_retrying(response.status()) {
                return Ok(None);
            }
            ensure!(response.ok(), "RPC HTTP error {}", response.status());
            Ok(Some(response.json().await?))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let response = reqwest::Client::new()
                .post(&self.url)
                .timeout(std::time::Duration::from_secs(30))
                .json(body)
                .send()
                .await?;
            if worth_retrying(response.status().as_u16()) {
                return Ok(None);
            }
            Ok(Some(response.error_for_status()?.json().await?))
        }
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let body = json!({"jsonrpc": "2.0", "id": "ckdutch", "method": method, "params": params});
        let mut result = None;
        // Resubmitting a signed transaction is safe: it keeps the same hash.
        for attempt in 0..6 {
            if attempt > 0 {
                pause(500 << attempt.min(4)).await;
            }
            if let Some(response) = self.attempt(&body).await? {
                result = Some(response);
                break;
            }
        }
        let result =
            result.context("The NEAR RPC endpoint is busy. Wait a moment and try again")?;
        if let Some(error) = result.get("error") {
            bail!("RPC: {error}");
        }
        result
            .get("result")
            .cloned()
            .context("RPC response is missing its result")
    }

    pub async fn view(&self, account: &str, method: &str, args: Value) -> Result<Value> {
        let result = self.request("query", json!({"request_type":"call_function", "finality":"final",
            "account_id": account_id(account)?, "method_name":method, "args_base64": STANDARD.encode(serde_json::to_vec(&args)?)})).await?;
        if let Some(error) = result.get("error") {
            bail!("Contract: {error}");
        }
        let bytes: Vec<u8> = serde_json::from_value(result["result"].clone())
            .context("Contract returned no bytes")?;
        serde_json::from_slice(&bytes).context("Contract returned invalid JSON")
    }

    pub async fn status(&self, account: &str) -> Result<SwitchStatus> {
        let result = self.view(account, "get_status", json!({})).await
            .context("Cannot read this switch. It must use the updated contract with get_status and the deadline fix")?;
        let status: SwitchStatus = serde_json::from_value(result)?;
        ensure!(
            status.version == "1" && status.owner == account,
            "Unsupported switch contract"
        );
        Ok(status)
    }

    pub async fn access_key(&self, signer: &Signer) -> Result<Value> {
        let result = self
            .request(
                "query",
                json!({"request_type":"view_access_key", "finality":"final",
            "account_id": signer.account, "public_key": signer.public_key()}),
            )
            .await?;
        ensure!(
            result["permission"] == "FullAccess",
            "A full-access key is required for account deployment and signing"
        );
        Ok(result)
    }

    pub async fn send(
        &self,
        signer: &Signer,
        receiver: &str,
        actions: Vec<Action>,
    ) -> Result<Outcome> {
        let access = self.access_key(signer).await?;
        let nonce = next_nonce(
            signer,
            access["nonce"]
                .as_u64()
                .context("Missing access-key nonce")?,
        )?;
        let block: [u8; 32] = bs58::decode(
            access["block_hash"]
                .as_str()
                .context("Missing block hash")?,
        )
        .into_vec()?
        .try_into()
        .map_err(|_| anyhow!("Invalid block hash"))?;
        let (signed_tx_base64, hash) = signer.sign(receiver, nonce, block, actions)?;
        // Awaiting the outcome in the submission itself keeps a transaction to a
        // single request, which matters on rate-limited public RPCs. Waiting for
        // finality rather than execution also means the `final`-block reads that
        // follow — an account, a nonce, a switch status — already see this
        // transaction. Only a server-side wait timeout falls back to polling.
        match self
            .request(
                "send_tx",
                json!({"signed_tx_base64": signed_tx_base64, "wait_until": "FINAL"}),
            )
            .await
        {
            Ok(result) => return outcome(result, hash),
            Err(error) if !format!("{error:#}").contains("TIMEOUT_ERROR") => {
                return Err(error).with_context(|| {
                    format!(
                        "Submission was not confirmed. Check transaction {hash} before retrying"
                    )
                })
            }
            Err(_) => {}
        }
        for _ in 0..60 {
            pause(3_000).await;
            if let Ok(result) = self
                .request(
                    "tx",
                    json!({"tx_hash": hash, "sender_account_id": signer.account, "wait_until": "NONE"}),
                )
                .await
            {
                if executed(&result) {
                    return outcome(result, hash);
                }
            }
        }
        bail!("Transaction {hash} is still unconfirmed. Check it in the explorer before submitting again")
    }

    pub async fn call(
        &self,
        signer: &Signer,
        receiver: &str,
        method: &str,
        args: Value,
    ) -> Result<Outcome> {
        self.send(signer, receiver, vec![call_action(method, args)?])
            .await
    }

    pub async fn mpc_key(&self) -> Result<String> {
        let state = self.view(MPC_CONTRACT, "state", json!({})).await?;
        let domains = state
            .pointer("/Running/keyset/domains")
            .and_then(Value::as_array)
            .context("MPC is not running")?;
        domains
            .iter()
            .find(|domain| domain["domain_id"] == DOMAIN_ID)
            .and_then(|domain| domain.pointer("/key/Bls12381/public_key"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .context("MPC CKD domain 2 is unavailable")
    }

    pub async fn derive_key(
        &self,
        signer: &Signer,
        contract: &str,
    ) -> Result<(Zeroizing<[u8; 32]>, String, String)> {
        // Refuse legacy contracts with the timestamp bug before obtaining a key.
        let status = self.status(contract).await?;
        ensure!(
            status.owner == signer.account || status.key_is_public,
            "The response window has not expired"
        );
        let mpc_key = self.mpc_key().await?;
        let ephemeral = Ephemeral::generate();
        let result = self
            .call(
                signer,
                contract,
                "request_confidential_key",
                json!({"app_public_key": {"AppPublicKey": ephemeral.public_key()}}),
            )
            .await?;
        let response: CkdResponse =
            serde_json::from_value(result.value).context("MPC did not return a CKD response")?;
        Ok((
            ephemeral.decrypt(&response, contract, &mpc_key)?,
            mpc_key,
            result.hash,
        ))
    }

    pub async fn deploy(
        &self,
        signer: &Signer,
        global: &str,
        delay_ms: u64,
        friends: Vec<String>,
    ) -> Result<Outcome> {
        ensure!(delay_ms > 0, "Choose a positive response window");
        for friend in &friends {
            account_id(friend)?;
        }
        let account = self.request("query", json!({"request_type":"view_account", "finality":"final", "account_id":signer.account})).await?;
        ensure!(
            account["code_hash"] == "11111111111111111111111111111111"
                && account
                    .get("global_contract_account_id")
                    .is_none_or(Value::is_null)
                && account
                    .get("global_contract_hash")
                    .is_none_or(Value::is_null),
            "This account already has a contract. Use a fresh dedicated account"
        );
        self.send(
            signer,
            &signer.account,
            vec![
                Action::UseGlobalContract {
                    contract_identifier: GlobalId::AccountId(account_id(global)?),
                },
                call_action(
                    "init",
                    json!({"challenge_delay_ms": delay_ms, "friends":friends}),
                )?,
            ],
        )
        .await
    }

    pub async fn create_account(&self, funder: &Signer, child: &Signer) -> Result<Outcome> {
        let suffix = format!(".{}", funder.account);
        let label = child
            .account
            .strip_suffix(&suffix)
            .context("New account must be a direct subaccount of the funding account")?;
        ensure!(
            !label.contains('.') && !label.is_empty(),
            "Choose a direct subaccount name"
        );
        self.send(
            funder,
            &child.account,
            vec![
                Action::CreateAccount,
                Action::Transfer {
                    deposit: 1_000_000_000_000_000_000_000_000,
                },
                child.add_key_action(),
            ],
        )
        .await
    }
}

/// The nonce to sign with, remembering what this session already submitted.
///
/// Both the nonce and the block hash come from the last final block, because an
/// optimistic block hash is rejected as expired. That nonce lags behind a
/// transaction that just landed, so the highest nonce used here wins.
fn next_nonce(signer: &Signer, on_chain: u64) -> Result<u64> {
    static USED: std::sync::Mutex<Option<std::collections::HashMap<String, u64>>> =
        std::sync::Mutex::new(None);
    let mut used = USED.lock().map_err(|_| anyhow!("Nonce tracking failed"))?;
    let used = used.get_or_insert_with(std::collections::HashMap::new);
    let key = format!("{}:{}", signer.account, signer.public_key());
    let nonce = on_chain
        .max(used.get(&key).copied().unwrap_or_default())
        .checked_add(1)
        .context("Nonce overflow")?;
    used.insert(key, nonce);
    Ok(nonce)
}

/// True once every receipt of the transaction has run, finalized or not.
fn executed(result: &Value) -> bool {
    matches!(
        result["final_execution_status"].as_str(),
        Some("EXECUTED_OPTIMISTIC" | "EXECUTED" | "FINAL")
    )
}

/// The transaction's own return value, or its failure.
fn outcome(result: Value, hash: String) -> Result<Outcome> {
    if let Some(error) = result["status"].get("Failure") {
        bail!("Transaction {hash} failed: {error}");
    }
    let encoded = result["status"]["SuccessValue"]
        .as_str()
        .with_context(|| format!("Transaction {hash} returned no result"))?;
    let bytes = STANDARD.decode(encoded)?;
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    Ok(Outcome { hash, value })
}

pub async fn pause(ms: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(ms).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_import_and_signed_transaction() {
        let signer = Signer::generate("owner.testnet").unwrap();
        let credentials: Value = serde_json::from_str(&signer.credentials().unwrap()).unwrap();
        let restored = Signer::import(
            "owner.testnet",
            credentials["private_key"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(restored.public_key(), signer.public_key());
        let (wire, hash) = signer
            .sign(
                "owner.testnet",
                1,
                [7; 32],
                vec![call_action("claim_owner_is_alive", json!({})).unwrap()],
            )
            .unwrap();
        let bytes = STANDARD.decode(wire).unwrap();
        let unsigned_len = bytes.len() - 65;
        assert_eq!(
            bs58::encode(Sha256::digest(&bytes[..unsigned_len])).into_string(),
            hash
        );
        let sig = ed25519_dalek::Signature::from_slice(&bytes[unsigned_len + 1..]).unwrap();
        signer
            .key
            .verifying_key()
            .verify_strict(&Sha256::digest(&bytes[..unsigned_len]), &sig)
            .unwrap();
        assert_eq!(
            borsh::to_vec(&Action::UseGlobalContract {
                contract_identifier: GlobalId::AccountId("global.testnet".into())
            })
            .unwrap()[..2],
            [10, 1]
        );
        assert!(Signer::import("owner.testnet", "ed25519:bad").is_err());
        assert!(account_id("owner.near").is_err());
    }
}
