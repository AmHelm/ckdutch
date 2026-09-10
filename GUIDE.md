# CKDutch setup and interaction guide

CKDutch is a dead man's switch built as a NEAR smart contract. Anyone can open
a challenge claiming that the owner is dead. The owner or one of their trusted
friends can cancel the challenge; otherwise, after the configured delay, a
caller can request a confidential key from NEAR MPC.

## Prerequisites

Enter the Nix development shell to get Rust, `cargo-near`, `near`, and
`ckd-example-cli`:

```console
nix develop
```

The NEAR accounts used below must exist on testnet, be funded, and have their
keys available to `near`.

## Configuration

Set the account that publishes the global contract, the account that owns its
contract instance, and two accounts used in the interaction examples:

```console
export GLOBAL_CONTRACT_ACCOUNT=flying-dman.testnet
export MY_USER_ACCOUNT=dont-kill-me-plz.testnet
export FRIEND_ACCOUNT=trusted-friend.testnet
export CLAIMANT_ACCOUNT=concerned-friend.testnet

# Demo value: ten seconds.
export TIMEOUT_MS=10000
```

`MY_USER_ACCOUNT` is both the deployed contract account and its owner. Use a
longer timeout for anything beyond a demo.

## Build and deploy

Build the WebAssembly contract:

```console
cargo near build non-reproducible-wasm \
  --manifest-path flying-dutchman/Cargo.toml
```

Publish it as a global contract:

```console
near contract deploy-as-global \
  use-file flying-dutchman/target/near/flying_dutchman.wasm \
  as-global-account-id "$GLOBAL_CONTRACT_ACCOUNT" \
  network-config testnet sign-with-keychain send
```

Deploy an instance and initialize its challenge delay. This example starts with
an empty friend list so that the friend-management flow below can be tried:

```console
near contract deploy "$MY_USER_ACCOUNT" \
  use-global-account-id "$GLOBAL_CONTRACT_ACCOUNT" \
  with-init-call init \
  json-args "{\"challenge_delay_ms\":$TIMEOUT_MS,\"friends\":[]}" \
  prepaid-gas '100 Tgas' attached-deposit '0 NEAR' \
  network-config testnet sign-with-keychain send
```

## Interact with the contract

All examples use NEAR testnet and the accounts configured above.

### Add or remove a trusted friend

Only the owner account can change the friend list:

```console
near contract call-function as-transaction \
  "$MY_USER_ACCOUNT" add_friend \
  json-args "{\"friend\":\"$FRIEND_ACCOUNT\"}" \
  prepaid-gas '30 Tgas' attached-deposit '0 NEAR' \
  sign-as "$MY_USER_ACCOUNT" \
  network-config testnet sign-with-keychain send
```

To remove that friend:

```console
near contract call-function as-transaction \
  "$MY_USER_ACCOUNT" remove_friend \
  json-args "{\"friend\":\"$FRIEND_ACCOUNT\"}" \
  prepaid-gas '30 Tgas' attached-deposit '0 NEAR' \
  sign-as "$MY_USER_ACCOUNT" \
  network-config testnet sign-with-keychain send
```

### Claim that the owner is dead

Any account can start a challenge:

```console
near contract call-function as-transaction \
  "$MY_USER_ACCOUNT" claim_owner_is_dead \
  json-args '{}' \
  prepaid-gas '30 Tgas' attached-deposit '0 NEAR' \
  sign-as "$CLAIMANT_ACCOUNT" \
  network-config testnet sign-with-keychain send
```

Only one challenge can be active at a time.

### Prove that the owner is alive

The owner or any configured friend can cancel the active challenge. This
example uses the trusted friend:

```console
near contract call-function as-transaction \
  "$MY_USER_ACCOUNT" claim_owner_is_alive \
  json-args '{}' \
  prepaid-gas '30 Tgas' attached-deposit '0 NEAR' \
  sign-as "$FRIEND_ACCOUNT" \
  network-config testnet sign-with-keychain send
```

### Request and decrypt the confidential key

The owner may request the key at any time. Other callers must first open a
challenge and wait for it to expire. The contract uses CKD domain `2` and the
fixed derivation path `d`.

First, query the MPC contract state and copy the BLS public key for
`domain_id: 2` from `Running.keyset.domains`:

```console
near contract call-function as-read-only \
  v1.signer-prod.testnet state json-args '{}' \
  network-config testnet now
```

In terminal 1, start the CKD helper. The signer ID must be the contract account,
because that account makes the cross-contract MPC request:

```console
export MPC_CKD_PUBLIC_KEY='bls12381g2:REPLACE_WITH_DOMAIN_2_PUBLIC_KEY'

ckd-example-cli \
  --mpc-ckd-public-key "$MPC_CKD_PUBLIC_KEY" \
  --domain-id 2 \
  --derivation-path d \
  --signer-account-id "$MY_USER_ACCOUNT"
```

The helper prints parameters containing a newly generated `AppPublicKey` and
then waits for a response. Keep it running and copy only the value of
`AppPublicKey`, for example `bls12381g1:...`.

In terminal 2, request the key. Use `MY_USER_ACCOUNT` as the signer for the
owner path, or `CLAIMANT_ACCOUNT` after an expired challenge:

```console
export REQUESTER_ACCOUNT="$MY_USER_ACCOUNT"
export APP_PUBLIC_KEY='bls12381g1:REPLACE_WITH_APP_PUBLIC_KEY'

near contract call-function as-transaction \
  "$MY_USER_ACCOUNT" request_confidential_key \
  json-args "{\"app_public_key\":{\"AppPublicKey\":\"$APP_PUBLIC_KEY\"}}" \
  prepaid-gas '100 Tgas' attached-deposit '0 NEAR' \
  sign-as "$REQUESTER_ACCOUNT" \
  network-config testnet sign-with-keychain send
```

Copy the returned JSON object containing `big_c` and `big_y` into the waiting
helper in terminal 1. It verifies and decrypts the MPC response, then prints:

```text
The key is: <64 hexadecimal characters>
```

The derived key is deterministic for the contract account and derivation path,
even though the helper generates a fresh app key for every request.

## Authorization summary

| Action | Who can call it? |
| --- | --- |
| Start a death challenge | Any account |
| Cancel a challenge | The owner or a configured friend |
| Add or remove a friend | The owner |
| Request the confidential key | The owner, or anyone after challenge expiry |

> **Implementation note:** the challenge deadline is stored in milliseconds,
> but the expiry check currently compares it with NEAR's nanosecond timestamp.
> As implemented, a newly opened challenge can therefore appear expired
> immediately. Use `env::block_timestamp_ms()` in the expiry check before
> relying on the configured delay.
