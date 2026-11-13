# QPC NEAR Policy Gate

A NEAR-native Rust prototype for QPC agent policy enforcement.

## Implemented in this prototype

- Owner-only agent policy configuration and revocation.
- Per-agent expiry, maximum payment amount, cumulative authorized cap.
- Recipient allowlist (maximum 64 entries per policy).
- Pause switch.
- Sequential per-agent nonce.
- Request hash format validation.
- Policy versioning and public audit logs.
- Unit tests for positive and adversarial paths.

## Important limitations

- **Authorization only:** `authorize_payment` reserves an amount against policy limits. It does not transfer NEAR or FT tokens and does not prove settlement.
- **Not private:** NEAR contract state, method arguments, and transaction metadata are public. This prototype does not provide shielded payments, ZK proofs, TEE execution, or x402.
- **No post-quantum verification:** no ML-DSA, SLH-DSA, or PQ signature verification is implemented.
- **No cross-chain execution:** other L1s require independent adapters and audits.
- The request hash is an audit binding, not proof that an off-chain payload is valid. The future settlement adapter must bind it to the exact network, asset, amount, recipient, expiry, and service, and must implement idempotency.

## Build and test

Requires Rust 1.95+ and the `wasm32-unknown-unknown` target (near-sdk 5.30.1 MSRV).

```sh
cargo test --manifest-path near/qpc-policy/Cargo.toml
cargo check --manifest-path near/qpc-policy/Cargo.toml --target wasm32-unknown-unknown --release
```

No deployment is performed by these commands.
