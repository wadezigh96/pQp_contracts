# QPC NEAR — Open Innovation Prototype

QPC (Quantum Persistence Contract) includes an independent NEAR-native Rust/near-sdk contract. It does not share bytecode or state with Ethereum or BNB Smart Chain.

## Prototype capabilities
- Human-owned versioned policies, pause and revocation.
- Exact agent, target, method and argument-byte hash checks.
- Per-call, cumulative and policy-wide deposit limits.
- Sequential nonces and unique request IDs.
- NEAR asynchronous promise execution and private callback.
- Failed target calls consume reserved budget; no automatic retry or refund.

## Build
```sh
cd near
cargo test --workspace
cargo check --workspace
cargo build --release --target wasm32-unknown-unknown -p qpc-near
```

## Safety
Initialize atomically with deployment by the intended owner. Never deploy uninitialized WASM and initialize later. This prototype is not audited or production-ready. Argument hashes bind bytes but do not validate semantic intent. Storage growth, callback recovery, refunds and target upgrades need further review. No PQ signature verification or quantum-resistance claim is implemented.
