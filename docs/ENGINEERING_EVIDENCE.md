# QPC engineering evidence — owner hardening

**Branch:** `reason/deliver-qpc-hardening-dr-d997d6a3`
**Scope:** local implementation and review draft only. No deployment, public transaction, key rotation, or external professional audit was performed.

## Owner initialization evidence

The NEAR contract initializer is now guarded by both `env::state_exists()` and an explicit owner check for `wkams.testnet`. Any existing state rejects reinitialization before replacement, and any other owner account rejects substitution before a supported state is created. `get_owner()` exposes the contract-storage owner for read-only verification, while `get_state_layout_version()` remains available for layout verification.

Administrative methods continue to authorize only `env::predecessor_account_id()` equal to the stored owner. Tests cover unauthorized `configure_agent`, `configure_envelope_profile`, recipient permission changes, and agent revocation. No owner-transfer method was added because WASM cannot control the NEAR account's FullAccess or deployment authority.

## Canonical fixture synchronization

The shared fixture `docs/fixtures/cross-l1-v1.json` now binds `cancellation.authorityId` to `wkams.testnet`. Its canonical JSON and commitment were regenerated with `stringifyCrossL1AuthorizationEnvelope` and `computeCrossL1AuthorizationCommitment`, not manually edited: `canonicalJson` matches byte-for-byte and the resulting `authorizationCommitment` is `c72c9a07dfd9ce8aecbd21c9c6bf858e5f089072a9ec84bafb03aadada653333`. The Rust fixture tests assert that cancellation authority equals `INTENDED_OWNER`; the offline VM predecessor constant and generated demo artifacts use the same owner/commitment.

## State and migration boundary

The root state begins with the `QPCNEAR\0LAYOUT\0\0` magic and layout version `1`. Missing, corrupt, unknown-version, truncated, and unversioned roots reject before collection handles are interpreted. No automatic migration or silent reset is implemented. No authentic historical deployed-state fixture was available, so no data transformation is claimed.

## Verification commands

Commands prepared for the hardening change:

```bash
cargo fmt --manifest-path near/qpc-near-agent-finance/Cargo.toml -- --check
cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked
cargo build --manifest-path near/qpc-near-agent-finance/Cargo.toml \
  --target wasm32-unknown-unknown --release
bash near/offline-vm/check.sh
```

**Current execution status:** BLOCKED in this sandbox because `cargo`, `rustc`, `rustfmt`, `rustup`, and NEAR Sandbox binaries are not installed. No offline VM, real NEAR Sandbox, WASM build, or deployed RPC test was represented as passing. The existing branch baseline records 29 NEAR unit tests; four owner-hardening tests were added, so the expected targeted count is 33 once the Rust toolchain is available. This count is not a pass claim.

## Sandbox requirement

A real local NEAR Sandbox test remains unresolved and must be run with disposable accounts only. Required scenarios are deployment, initialization with `wkams.testnet` as the intended storage owner, unauthorized owner substitution, reinitialization, owner-only admin calls, failed-call rollback, state persistence, gas/storage behavior, and account-level upgrade authority review. If Sandbox cannot be provisioned, the result must remain **BLOCKED**.

## Account-level authority

`funzest6203.testnet` account deployment authority and any FullAccess key are outside the contract-storage owner boundary. A contract method cannot revoke or constrain that authority. The documented upgrade procedure therefore requires two-person review of the exact WASM hash, layout version, migration plan, owner, deployment account and transaction parameters, followed by read-only verification. No key is requested, stored, printed, rotated, or transmitted.

## Claims not supported

- No production readiness or live payment claim.
- No post-quantum readiness claim.
- No claim of a professional external audit.
- No claim that unit tests or offline VM are equivalent to a real NEAR Sandbox or deployed receipt graph.
- No deployment to `funzest6203.testnet` and no public transaction submission.
