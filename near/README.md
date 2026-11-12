# PQP NEAR Core

This workspace contains the first native NEAR implementation slice of PQP.

## Scope

The current contract implements a fail-closed policy registry and one-time action authorization:
- owner-controlled policy create/update/pause/revoke;
- immutable agent identity per policy;
- policy versions and bounded validity windows;
- exact target, method and 32-byte argument-hash matching;
- per-call, per-action cumulative and policy-wide deposit limits;
- sequential nonces and unique request IDs;
- authorization and usage events.

**This milestone does not execute cross-contract calls or transfer assets.** An authorization receipt is not proof that a target call ran. Do not use this contract as a production custody or execution layer. The next milestone must add an explicit NEAR promise adapter with authenticated callbacks, idempotent result handling, failure accounting, and integration tests.

## Build and test

Requires Rust 1.95.0 and the wasm32 target.

```sh
cd near
cargo test --workspace
cargo check --workspace
cargo build --release --target wasm32-unknown-unknown -p pqp-near
```

No deployment or wallet interaction is part of this workspace.
