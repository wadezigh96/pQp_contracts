# PQP NEAR Core

This workspace contains the first native NEAR implementation slice of PQP.

## Scope

The current contract implements a fail-closed policy registry and constrained asynchronous action execution:
- owner-controlled policy create/update/pause/revoke;
- immutable agent identity per policy;
- policy versions and bounded validity windows;
- exact target and method matching, plus a SHA-256 check over the exact argument bytes sent to the target;
- per-call, per-action cumulative and policy-wide deposit limits;
- sequential nonces and unique request IDs;
- authorization and usage events.

`dispatch_action` now dispatches one policy-approved NEAR function call and chains a private `on_action_result` callback. It forwards only the exact argument bytes whose digest is committed by the policy. The target call is funded only by the exact attached deposit supplied to PQP. Failed calls still consume the reserved budget and nonce to prevent retry-based quota bypass. Oversized successful return data is distinguished from target failure. This is a constrained execution prototype, not a production custody layer: test against a local NEAR runtime and testnet, measure gas/storage, review method-specific argument schemas, and obtain independent security review before real assets.

## Build and test

Requires Rust 1.95.0 and the wasm32 target.

```sh
cd near
cargo test --workspace
cargo check --workspace
cargo build --release --target wasm32-unknown-unknown -p pqp-near
```

No deployment or wallet interaction is part of this workspace.
