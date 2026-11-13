# ZecBlock Private PIR upstream audit

Audit date: 2026-11-13
QPC branch: `manus/continue-near-zcash`
Audited upstream repository: https://github.com/Kenbak/zecblock
Audited upstream commit: `41ac8b2b2a9dff856486a9d229d748dcb8328068`
Audited path: `prototype/pir-browser`

## Decision

**Do not integrate this prototype as a production QPC transport yet.** It is useful as a source-pinned feasibility implementation and test oracle, but its own documentation explicitly says it is local-only and does not implement the production wallet-PIR HTTP envelope, private address history, or mempool retrieval.

QPC's `privacy_query` module therefore remains a bounded opaque-ciphertext transport seam. It is not a PIR cryptographic implementation and must not be described as one.

## Reproducibility inputs

The upstream `prototype/pir-browser/bootstrap.py` pins and checks the following archives:

| Dependency | Repository | Revision | SHA-256 |
|---|---|---|---|
| wallet-libraries | `zakura-core/wallet-libraries` | `a7c842fd4f16790ed6739346f44557a97065d242` | `a424889b2a26c24c6416bbc085fbc7fdf35fa82e7fe35e695cd4cd7726927361` |
| ipir-sp | `valargroup/ipir-sp` | `5ea080a3281a01c5459f585e0a1bbbde50ec2bd2` | `2f8145e0aaf9ad201aafcd2843dc39cb71d90422374f5e4c15074debf51b819c` |

The upstream prototype also pins `Cargo.lock` and requires Rust 1.97.1, target `wasm32-unknown-unknown`, Python 3.12+, and `wasm-bindgen-cli 0.2.127`. The current QPC workflow uses Rust 1.89, so the upstream build must run in a separate job/toolchain rather than changing the wallet adapter's compiler globally.

The bootstrap downloads source archives at the pinned revisions and verifies archive SHA-256 plus extracted source-file equality. A QPC adoption job should use the same immutable inputs and verify checksums before building. Do not use floating `main` or unpinned git dependencies.

## Protocol and trust boundaries

- The basic browser fixture is a bounded local 2,048-row transaction-directory fixture, not a production anonymity-set qualification.
- The full-confirmed-snapshot mode supports a fixed 1–64 chunk plan. The host must generate every query before decoding any response; query scheduling is part of the privacy contract.
- The prototype README explicitly says it does not implement the production wallet-PIR HTTP envelope, private address history, or mempool retrieval.
- The browser verifies signed publication metadata and authenticated PIR rows according to the prototype's publication format. That does not establish that publisher data is complete or that a transaction is included in canonical Zcash consensus.
- Recomputed raw transaction IDs prove byte identity, not block inclusion, prevout-derived fee correctness, spent state, indexer enrichment correctness, or complete coverage.
- Freshness, generation retention, transport scheduling, IP/timing exposure, and no-fallback behavior remain host responsibilities.
- PIR query privacy is distinct from Zcash shielded-payment privacy and from settlement/finality verification.

QPC must continue to treat PIR output as an explorer observation. A trusted Zcash light client/node must independently verify transaction inclusion and confirmations before QPC reports settlement.

## License discrepancy — adoption blocker

At the audited upstream commit, the root `LICENSE` file contains the MIT License text, while the root `README.md` says the project is licensed under AGPL-3.0 with a Commons Clause restriction. These statements conflict. The prototype also bootstraps source from two other repositories with their own license terms.

Before copying, vendoring, modifying, or distributing upstream code in QPC, obtain a definitive license clarification from the upstream maintainers and review the licenses of both pinned dependency archives and transitive dependencies. This audit does not make a legal determination and does not grant permission to reuse code.

## QPC adoption gates

1. Resolve the upstream license discrepancy and dependency license inventory.
2. Reproduce upstream native and WASM tests in an isolated CI job using the pinned toolchain and exact archive checksums.
3. Obtain a documented production PIR publication and transport contract from ZecBlock; do not reverse-engineer or invent a production endpoint from the local fixture harness.
4. Implement a complete fixed-plan transport with strict byte limits, publication identity pinning, timeouts, replay handling, and fail-closed behavior.
5. Test response tampering, chunk reorder/omission, stale publications, malicious manifests, cancellation, and transport metadata leakage.
6. Independently verify chain inclusion and confirmations using a trusted Zcash light client/node.
7. Keep PIR query material separate from seeds, viewing keys, shielded note state, and signing.

## QPC implementation status

- ZIP-316 Unified Address parsing and network validation: implemented in the QPC adapter.
- Shielded-first receiver selection: implemented as policy logic.
- Testnet SQLite wallet storage initialization: implemented as storage-only foundation.
- Opaque encrypted payload transport interface: implemented, but no vendor wire protocol or cryptographic client is wired in.
- ZecBlock production PIR integration: not implemented.
- Shielded note synchronization, transaction construction/signing, broadcast, and independent settlement verification: not implemented.
- Mainnet transaction or funds transfer: not performed.
