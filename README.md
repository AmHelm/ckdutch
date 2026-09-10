# CKDutch — revealing your secrets when you are no longer able to

CKDutch is a dead man's switch built as a NEAR smart contract. If the owner can
no longer respond to a challenge, the contract makes a confidential key
available through NEAR MPC.

For setup details and examples of every contract interaction, see the
[standalone guide](GUIDE.md).

## Browser app

The frontend uses TypeScript, React, `near-kit` 0.20.0 and NEAR Connect with
Intear or Meteor on testnet. Its small Rust/WASM module preserves the original capsule
format and verifies MPC responses. Account keys stay in the wallet; the app asks
the wallet to approve setup and each contract call.

**Wallet support (verified September 10, 2026):** Intear's standard web wallet
successfully attached the global contract, initialized a dedicated testnet
switch, and approved the MPC request used to seal a capsule. Choose Intear for
new-switch setup. Meteor connects and restores sessions, but its published
executor rejects global-contract actions; its wallet interface also explicitly
refuses attachment when tested with an unchanged build of its newer source.
This app uses the standard published connectors and does not ship wallet patches.

Install [Bun](https://bun.sh/) and Rust with the `wasm32-unknown-unknown` target,
then run:

```console
cd frontend
bun install --frozen-lockfile
bun run build:crypto
bun run dev
```

Open the local URL printed by Vite. Create and fund a **dedicated testnet
account in Intear**, connect it, and use **Set up a switch**. Setup attaches
the shared contract and initializes it in one transaction; accounts that
already have contract code are rejected. The switch account itself is its
owner. Connecting a parent account does not grant control of a child switch.

To check or build the frontend:

```console
bun run typecheck
bun run test
bun run test:crypto
bun run build
bun run preview
```

`bun run build` regenerates the WASM module, checks TypeScript and creates
`frontend/dist`. Deploy that directory as a static site over HTTPS. No server
session, NEP-413 sign-in proof, application-held account key, or automatic
function-call permission is needed for these wallet actions.

For a manual testnet smoke test, connect Intear, set up a fresh account,
seal and download a small capsule, then upload and open it as the owner.
Check in, manage a trusted friend, and test the challenge/recovery flow with
a short response window and a second account. Every write requires wallet
approval. If submission is uncertain, inspect the displayed transaction link
before retrying.

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
