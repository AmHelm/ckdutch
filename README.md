# CKDutch — revealing your secrets when you are no longer able to

CKDutch is a dead man's switch built as a NEAR smart contract. If the owner can
no longer respond to a challenge, the contract makes a confidential key
available through NEAR MPC.

For setup details and examples of every contract interaction, see the
[standalone guide](GUIDE.md).

## How to build and deploy

### Prepare

Enter the development shell and configure the testnet accounts:

```console
nix develop

export GLOBAL_CONTRACT_ACCOUNT=flying-dman.testnet
export MY_USER_ACCOUNT=dont-kill-me-plz.testnet
export TIMEOUT_MS=10000
```

### Build

```console
cargo near build non-reproducible-wasm \
  --manifest-path flying-dutchman/Cargo.toml
```

### Deploy the global contract

```console
near contract deploy-as-global \
  use-file flying-dutchman/target/near/flying_dutchman.wasm \
  as-global-account-id "$GLOBAL_CONTRACT_ACCOUNT" \
  network-config testnet sign-with-keychain send
```

### Instantiate the global contract

```console
near contract deploy "$MY_USER_ACCOUNT" \
  use-global-account-id "$GLOBAL_CONTRACT_ACCOUNT" \
  with-init-call init \
  json-args "{\"challenge_delay_ms\":$TIMEOUT_MS,\"friends\":[]}" \
  prepaid-gas '100 Tgas' attached-deposit '0 NEAR' \
  network-config testnet sign-with-keychain send
```
