# CKDutch - revealing your secerets when you're not able to.

# How to build and deploy
Prep:
```
export GLOBAL_CONTRACT_ACCOUNT=flying-dman.testnet
```

```
export MY_USER_ACCOUNT=dont_kill_me_plz.testnet
```

```
export TIMEOUT_MS=10000
```

Build:
```
cargo near build non-reproducible-wasm --manifest-path flying-dutchman/Cargo.toml 
```

Deploy global contract:
```
near contract deploy-as-global use-file flying-dutchman/target/near/flying_dutchman.wasm as-global-account-id $GLOBAL_CONTRACT_ACCOUNT network-config testnet sign-with-keychain send
```

Instantiate global contract:
```
near contract deploy $MY_USER_ACCOUNT use-global-account-id $GLOBAL_ACCOUNT_ID with-init-call init json-args '{"challenge_delay_ms": $TIMEOUT_MS, "friends": []}' prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' network-config testnet sign-with-keychain send
```
