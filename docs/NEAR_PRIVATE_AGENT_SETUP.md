# NEAR Private Agent — safe setup and verification

## Current capability and boundaries

The NEAR contract supports owner-pinned initialization, owner-managed agent policies, native predecessor authorization, replay protection, and a narrow same-chain NEAR testnet transfer prototype. Authorization alone is not execution or cross-chain settlement. The repository workflow can build and execute contract WASM in a pinned **offline VM** with mocked host context; it does not run a nearcore Sandbox or deploy a contract.

The intended owner fixture is exactly `wkams.testnet`. `funzest6203.testnet` is only the intended future deployment target; do not deploy or submit a public Testnet transaction without separate explicit approval. For any future local Sandbox, use disposable local accounts and no production credentials.

## Prerequisites for local checks

- Node.js 22 and npm.
- Rust/Cargo plus the `wasm32-unknown-unknown` target for build/VM checks.
- A native C build toolchain for offline VM dependencies.
- A local NEAR Sandbox binary is additionally required for genuine Sandbox integration; the repository currently has no Sandbox test harness or installer script.

Check availability from the repository root:

```bash
node --version
npm --version
rustc --version
cargo --version
rustup target list --installed
command -v near-sandbox || true
```

## Reproducible checks available in CI

```bash
cargo fmt --manifest-path near/qpc-near-agent-finance/Cargo.toml -- --check
cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked
cargo build --manifest-path near/qpc-near-agent-finance/Cargo.toml \
  --target wasm32-unknown-unknown --release
npm ci --prefix near/offline-vm --ignore-scripts --no-audit --no-fund
CARGO_BUILD_JOBS=1 bash near/offline-vm/check.sh
```

The offline VM uses the actual built WASM, but mocks account state, host calls, promise outcomes, and execution context. It performs no signing, RPC, deployment, native transfer, or consensus verification. Do not report its five tests as NEAR Sandbox tests.

## Initialization and authority

The contract initializer rejects existing state, requires `predecessor_account_id` to equal the owner argument, and pins that argument to `wkams.testnet`. This is contract-storage ownership. NEAR account access keys, including FullAccess and deployment authority, are controlled at the account layer and cannot be revoked by a contract method. Never give an agent/router FullAccess, change Owner keys, or include keys/seeds in a repository or command.

## Not currently provided

There is no project script that starts local Sandbox, creates local accounts, deploys WASM, signs test calls, or executes a NEAR ↔ Zcash route. The testnet-only transfer prototype must not be mistaken for a cross-chain bridge. For an actual local integration test, a Sandbox harness, local account provisioning, safe key custody, and a separately implemented Zcash backend are prerequisites.
