# CKDutch - revealing your secerets when you're not able to.

# How to build and deploy
Prep:
```
export MY_ACCOUNT=flying-dman.testnet
```

```
export MY_USER_ACCOUNT=dont_kill_me_plz.testnet
```

Build:
```
cargo near build non-reproducible-wasm --manifest-path flying-dutchman/Cargo.toml 
```

Deploy global contract:
```
near contract deploy-as-global use-file flying-dutchman/target/near/flying_dutchman.wasm as-global-account-id flying-dman.testnet network-config testnet sign-with-keychain send
```

Instantiate global contract:
```
```
