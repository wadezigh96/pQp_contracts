# Current status on branch `reason/deliver-qpc-hardening-dr-d997d6a3`

Last updated: 2026-11-14

## Verification evidence

| Area | Result | Scope |
|---|---:|---|
| JavaScript agent-router | **517/517 PASS** | Fresh local complete unit-test run. |
| Cross-L1 envelope/replay tests | **60/60 PASS** | Fresh local focused suite; canonical authorization commitment unchanged. |
| Replay-store recovery | **10/10 PASS** | Includes fail-closed handling of a persisted V1 replay index under V2. |
| Optional hybrid-auth prototype | **26/26 PASS** | Fresh local run with lockfile-pinned dependencies. |
| NEAR Rust unit tests | **32/32 PASS** | GitHub Actions on final head `5a523cbf9f8d46152bdcaaf9004ebd185829ffbd`; [run 43546720154](https://github.com/wadezigh96/pQp_contracts/actions/runs/43546720154). |
| NEAR offline WASM VM | **5/5 PASS** | Separate mocked-host/offline-VM suite in [run 43546720154](https://github.com/wadezigh96/pQp_contracts/actions/runs/43546720154); this is not NEAR Sandbox. |
| Zcash adapter unit tests | **46/46 PASS** | GitHub Actions [run 43546720130](https://github.com/wadezigh96/pQp_contracts/actions/runs/43546720130); not regtest. |
| PR #1 required/status checks | **10/10 PASS** | All listed PR checks successful on final head `5a523cb`; [PR #1](https://github.com/wadezigh96/pQp_contracts/pull/1). |
| NEAR Sandbox ↔ Zcash regtest demo | **BLOCKED / NOT RUN** | Missing local runtimes and missing shielded-payment, wallet-sync, and independent receipt-verification implementation. |

The historical **29 baseline / 33 expected** NEAR count was incorrect. Repository history supports 27 tests before owner hardening, then 31 after four owner tests. This branch adds one predecessor-authorization regression, for 32 source tests; the final-head CI reports all 32 passing.

The CI run above executed a compatible WASM build/offline VM test suite. It does **not** establish a nearcore Sandbox deployment, signed transaction, live receipt graph, or Zcash payment. `cargo`, `rustc`, `rustfmt`, `near-sandbox`, `zcashd`, `zebrad`, `lightwalletd`, and Docker are absent locally. The CI Rust result is the evidence for current Rust source tests; local Rust tests were not run.

## Workstream status

| Workstream | Status | Notes |
|---|---|---|
| NEAR owner initialization | IMPLEMENTED; 32/32 Rust tests pass in current code-revision CI | Requires empty state, predecessor equal to supplied owner, and exact owner `wkams.testnet`; regression rejects an attacker even when the attacker supplies the intended owner argument. |
| NEAR state migration | NOT IMPLEMENTED; fails closed | Unsupported layout headers reject. There is no contract `migrate` method or migration test suite, and no authentic historical deployed-state fixture. |
| Cross-L1 replay domain | IMPLEMENTED; JS tests pass | V2 key binds source adapter identity. A V1 persisted index is rejected fail-closed; no persistent-store migration is implemented. This is a release gate before upgrading an existing store. |
| Zcash shielded payments | NOT IMPLEMENTED | Address parsing, receiver policy, replay guard, local wallet DB, and selected read-only probes exist; transaction construction/signing/broadcast, wallet sync, and independent receipt verification do not. |
| Post-quantum protocol | PROTOTYPE / NOT END-TO-END | Optional off-chain Ed25519 + ML-DSA-65 hybrid verifier exists. ML-KEM, full protocol/key lifecycle, and on-chain enforcement are not implemented. |
| NEAR Sandbox | BLOCKED / NOT RUN | No local Rust toolchain or `near-sandbox`; CI offline VM is a separate mocked-host check. |
| Zcash regtest | BLOCKED / NOT RUN | No local Zcash node/lightwalletd/Docker and no implemented outbound/receive flow. |
| Intended deployment account | NOT DEPLOYED | `funzest6203.testnet` remains only the intended target. No public account, key, deployment, or transaction was modified. |

No production readiness, live payments, public cross-chain settlement, active post-quantum security, or external-audit claims are made. No Mainnet or public Testnet deployment or transaction was attempted.
