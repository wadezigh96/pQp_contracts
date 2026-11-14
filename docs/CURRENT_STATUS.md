# Current status on branch `reason/deliver-qpc-hardening-dr-d997d6a3`

Last updated: 2026-11-14

## Current working-tree verification

| Area | Result | Scope |
|---|---:|---|
| JavaScript agent-router | **517/517 PASS** | Fresh local run on the modified source; unit tests only. |
| Cross-L1 envelope/replay tests | **60/60 PASS** | Fresh local focused run; canonical fixture remains unchanged. |
| Optional hybrid-auth prototype | **26/26 PASS** | Fresh local run after installing lockfile-pinned npm dependencies. |
| NEAR Rust tests | **32 test declarations; execution pending current-revision CI** | Starting-revision CI ran 31. This branch adds one predecessor-authorization regression. Cargo/rustc are absent locally. |
| NEAR offline VM | **5/5 PASS on starting revision only** | Current-revision workflow must be checked after push. Offline VM is not Sandbox. |
| Zcash adapter | **46/46 PASS on starting revision only** | CI unit tests; no Zcash regtest or transaction. Local Cargo unavailable. |
| NEAR Sandbox ↔ Zcash regtest demo | **BLOCKED / NOT RUN** | Missing node/toolchain and missing shielded transaction/sync/receipt-verification implementation. |

The historical **29 baseline / 33 expected** NEAR count was incorrect. Source history supports 27 tests before the owner-hardening tests, then 31 after four were added. The current source has 32 test declarations after this change adds one unauthorized-predecessor regression test. Only actual CI output for this revision can establish how many pass.

## Workstream status

| Workstream | Status | Notes |
|---|---|---|
| NEAR owner initialization | IMPLEMENTED; current-revision CI pending | Requires empty state, caller equal to supplied owner, and exact owner `wkams.testnet`; test forbids attacker initialization with the correct owner argument. |
| NEAR state migration | NOT IMPLEMENTED; fails closed | Unsupported layout headers reject. There is no contract `migrate` method or migration test suite, and no authentic historical deployed-state fixture. |
| Cross-L1 replay domain | IMPLEMENTED; JS tests pass | V2 key binds source adapter identity; a V1 persisted index is rejected fail-closed. No persistent-store migration is implemented. Authorization commitment remains the required literal. |
| Zcash shielded payments | NOT IMPLEMENTED | Address parsing, receiver policy, replay guard, local wallet DB and selected read-only probes exist; no transaction construction/signing/broadcast, scanning/sync, or receipt proof. |
| Post-quantum protocol | PROTOTYPE / NOT END-TO-END | Optional off-chain Ed25519 + ML-DSA-65 hybrid verifier exists. ML-KEM, full protocol/key lifecycle, and on-chain enforcement are not implemented. |
| NEAR Sandbox | BLOCKED / NOT RUN | No local Rust toolchain or near-sandbox; CI offline VM is a separate mocked-host check. |
| Zcash regtest | BLOCKED / NOT RUN | No local Zcash node/lightwalletd/Docker and no implemented outbound/receive flow. |
| Intended deployment account | NOT DEPLOYED | `funzest6203.testnet` remains only the intended target. No public account, key, deployment, or transaction was modified. |

No production readiness, live payments, public cross-chain settlement, active post-quantum security, or external-audit claims are made.
