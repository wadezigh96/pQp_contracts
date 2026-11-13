# QPC NEAR Policy Gate

A NEAR-native Rust prototype for QPC agent policy enforcement.

## Implemented in this prototype

- Owner-only agent policy configuration and revocation.
- Per-agent expiry, maximum payment amount, cumulative reserved + settled cap.
- Recipient allowlist (maximum 64 entries per policy).
- Pause switch.
- Sequential per-agent nonce.
- Contract-derived SHA-256 payment-intent digest.
- Policy versioning and public audit logs.
- Two-step owner rotation (propose + accept).
- Separate emergency guardian that can pause but cannot unpause or edit policy.
- Contract-derived SHA-256 intent digest with deployment-domain and contract-account separation.
- Lifetime cap of 256 unique agent policies to bound state growth.
- Explicit payment reservation records with `Pending`, `Settled`, and `Released` states.
- Adapter-gated settlement finalization, unique settlement-reference replay protection, and owner/adapter release path.
- Pending amounts count against the policy cap; settled amounts remain consumed; release returns capacity.
- Reconfiguration is blocked until all pending reservations are released or settled.
- Unit tests for positive and adversarial paths.

## Important limitations

- **Accounting lifecycle only:** `authorize_payment` reserves an amount; `settle_payment` records a settlement attested by the configured adapter; `release_payment` frees a pending reservation. None of these methods transfers NEAR or FT tokens. The adapter's verification remains a trusted external boundary.
- **Not private:** NEAR contract state, method arguments, and transaction metadata are public. This prototype does not provide shielded payments, ZK proofs, TEE execution, or x402.
- **No post-quantum verification:** no ML-DSA, SLH-DSA, or PQ signature verification is implemented.
- **No cross-chain execution:** other L1s require independent adapters and audits.
- The current intent digest binds deployment domain, contract account, agent, policy version, nonce, recipient, and amount. It does not yet include token/asset, service, payment network, or settlement expiry. Those fields must be added before multi-asset or cross-chain use.
- Owner authority is only as secure as the NEAR account controlling it. Use a properly configured multisig account as owner for production; QPC does not implement threshold signatures internally.
- The guardian is a pause-only role. It cannot unpause, configure agents, rotate ownership, or authorize payments.
- Settlement finalization is only as trustworthy as the configured adapter. Do not assign an adapter until its proof verification and replay behavior are independently reviewed.
- The reservation map has no pruning/retention mechanism yet. State growth and storage-deposit economics must be addressed before production.

## Build and test

Requires Rust 1.95+ and the `wasm32-unknown-unknown` target (near-sdk 5.30.1 MSRV).

```sh
cargo test --manifest-path near/qpc-policy/Cargo.toml
cargo check --manifest-path near/qpc-policy/Cargo.toml --target wasm32-unknown-unknown --release
```

No deployment is performed by these commands.
