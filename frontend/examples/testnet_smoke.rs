//! Explicit, paid testnet integration test. No real plaintext or keys are logged.
//! Usage: cargo run --example testnet_smoke -- CREDENTIALS WASM [FUNDING_ACCOUNT]
//! Creates isolated accounts; reuses the supplied development key for recoverability.
//! Publishing global code costs ~13.3 NEAR per run, so set CKDUTCH_GLOBAL to an
//! account that already publishes this exact WASM to skip that step.
use anyhow::{ensure, Context, Result};
use ckdutch_ui::{
    crypto::{self, Ephemeral},
    near::{pause, Action, GlobalDeployMode, Rpc, Signer},
};
use serde_json::{json, Value};
use zeroize::Zeroizing;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    ensure!(
        args.len() >= 3,
        "Provide a credentials JSON path and compiled contract WASM path"
    );
    let credentials = Zeroizing::new(std::fs::read_to_string(&args[1])?);
    let value: Value = serde_json::from_str(&credentials)?;
    let funding = args
        .get(3)
        .map(String::as_str)
        .or_else(|| value["account_id"].as_str())
        .context("Provide the funding account")?;
    let secret = Zeroizing::new(
        value["private_key"]
            .as_str()
            .context("Missing private_key")?
            .to_string(),
    );
    let funder = Signer::import(funding, &secret)?;
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let reused_global = std::env::var("CKDUTCH_GLOBAL")
        .ok()
        .filter(|g| !g.is_empty());
    let publisher = Signer::import(
        &reused_global
            .clone()
            .unwrap_or_else(|| format!("ckdui-{suffix}.{funding}")),
        &secret,
    )?;
    let owner = Signer::import(&format!("ckdsw-{suffix}.{funding}"), &secret)?;
    let code = std::fs::read(&args[2])?;
    ensure!(
        code.len() < 150_000,
        "Smoke test budget expects a contract under 150 KB"
    );
    let rpc = Rpc::default();
    rpc.access_key(&funder).await?;
    if reused_global.is_some() {
        println!(
            "Reusing published global contract {} and creating switch {} (1 NEAR funding).",
            publisher.account, owner.account
        );
    } else {
        println!(
            "Creating isolated global publisher {} and switch {} (17 NEAR total funding).",
            publisher.account, owner.account
        );
        let created = rpc.create_account(&funder, &publisher).await?;
        println!("Publisher created: {}", created.hash);
        rpc.send(
            &funder,
            &publisher.account,
            vec![Action::Transfer {
                deposit: 15_000_000_000_000_000_000_000_000,
            }],
        )
        .await?;
        let deployed = rpc
            .send(
                &publisher,
                &publisher.account,
                vec![Action::DeployGlobalContract {
                    code,
                    deploy_mode: GlobalDeployMode::AccountId,
                }],
            )
            .await?;
        println!("Global contract published: {}", deployed.hash);
    }
    rpc.create_account(&funder, &owner).await?;
    let deployed = rpc
        .deploy(
            &owner,
            &publisher.account,
            60_000,
            vec![funder.account.clone()],
        )
        .await?;
    println!(
        "Global contract attached and initialized: {}",
        deployed.hash
    );
    ensure!(
        !rpc.status(&owner.account).await?.key_is_public,
        "Fresh key must be private"
    );
    let (key, mpc, owner_hash) = rpc.derive_key(&owner, &owner.account).await?;
    println!("Owner CKD pairing verified: {owner_hash}");
    let capsule = crypto::encrypt(
        &key,
        &owner.account,
        &mpc,
        "smoke.txt",
        b"CKDutch testnet recovery test",
    )?;
    let public_request =
        || json!({"app_public_key":{"AppPublicKey":Ephemeral::generate().public_key()}});
    let error = rpc
        .call(
            &funder,
            &owner.account,
            "request_confidential_key",
            public_request(),
        )
        .await
        .err()
        .context("Public request succeeded without challenge")?;
    ensure!(
        error.to_string().contains("Only the owner"),
        "Unexpected rejection: {error}"
    );
    println!("Public key request rejected without a challenge.");
    rpc.call(&funder, &owner.account, "claim_owner_is_dead", json!({}))
        .await?;
    let error = rpc
        .call(
            &funder,
            &owner.account,
            "request_confidential_key",
            public_request(),
        )
        .await
        .err()
        .context("Public request succeeded before deadline")?;
    ensure!(
        error.to_string().contains("Only the owner"),
        "Unexpected rejection: {error}"
    );
    println!("Public key request rejected during response window.");
    rpc.call(&funder, &owner.account, "claim_owner_is_alive", json!({}))
        .await?;
    ensure!(
        rpc.status(&owner.account)
            .await?
            .challenge_deadline_ms
            .is_none(),
        "Friend check-in did not cancel"
    );
    println!("Trusted friend successfully cancelled challenge.");
    let challenge = rpc
        .call(&funder, &owner.account, "claim_owner_is_dead", json!({}))
        .await?;
    println!(
        "New challenge: {}. Waiting for chain deadline…",
        challenge.hash
    );
    let mut released = false;
    for _ in 0..45 {
        if rpc.status(&owner.account).await?.key_is_public {
            released = true;
            break;
        }
        pause(2_000).await;
    }
    ensure!(released, "Release deadline not reached within test timeout");
    let (recovered, recovered_mpc, hash) = rpc.derive_key(&funder, &owner.account).await?;
    ensure!(
        *key == *recovered && mpc == recovered_mpc,
        "Owner and public CKD keys differ"
    );
    ensure!(
        crypto::decrypt(&recovered, &capsule)?.as_slice() == b"CKDutch testnet recovery test",
        "Wrong plaintext"
    );
    println!("PASS: public CKD pairing verified, key matches, capsule authenticated and decrypted: {hash}");
    println!(
        "GLOBAL_CONTRACT={}\nSWITCH_ACCOUNT={}",
        publisher.account, owner.account
    );
    Ok(())
}
