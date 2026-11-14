# Zcash Private Agent — safe setup and verification

## Current capability and boundaries

The adapter uses official Rust Zcash libraries to parse ZIP-316 Unified Addresses, enforce shielded receiver policy, generate opaque recipient references, persist a local wallet database/replay state, and validate selected read-only lightwalletd responses. Current wallet/database code is not a transaction-capable regtest wallet.

The repository does **not** currently construct or sign shielded transactions, broadcast them, synchronize/scan for received notes, or independently prove transaction inclusion, confirmations, or shielded receipt. The router route returns `NOT_IMPLEMENTED` for native shielded settlement. Therefore no NEAR ↔ Zcash payment can be demonstrated merely by starting a Zcash node.

## Prerequisites for adapter tests

- Rust/Cargo and a native build toolchain.
- No wallet seed, spending key, viewing key, RPC credential, or live network is required for unit tests.

From the repository root:

```bash
cargo fmt --manifest-path zcash/qpc-zcash-agent/Cargo.toml -- --check
cargo test --manifest-path zcash/qpc-zcash-agent/Cargo.toml --locked
cargo check --manifest-path zcash/qpc-zcash-agent/Cargo.toml --locked
```

These are address/policy/storage/adapter tests. They are not Zcash regtest or Testnet transactions.

## Regtest and privacy safety

A future deterministic integration suite must use an isolated regtest node and disposable wallet, verify the selected chain/network identity, require Orchard or Sapling recipients under shielded-only policy, and check the returned transaction ID and received note/confirmation through an independent wallet/node interface. Transparent-only fallback is not an acceptable private-payment substitute.

Do not put seeds, spending keys, viewing keys, wallet backups, raw shielded addresses, or node credentials into agent prompts, logs, CI artifacts, or committed files. Use restrictive local filesystem permissions for wallet databases and backups. Never use Mainnet or real funds for this demo.

## Current blockers

This execution environment has no `zcashd`, `zebrad`, `lightwalletd`, or Docker. More importantly, the corresponding transaction, broadcast, synchronization, and receipt-verification backend is not implemented in the repository. Zcash regtest and Zcash Testnet transaction flows are therefore **BLOCKED / NOT RUN**; no transaction ID exists to report.
