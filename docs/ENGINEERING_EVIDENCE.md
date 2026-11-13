# QPC engineering evidence and requirement traceability

## Current verification on PR branch (2026-11-14)

On branch `reason/deliver-qpc-hardening-dr-d997d6a3` (commit 1d7cc5e8acd178fc2c02d9bb7d0b9c87073b3dab):

- PASS: `node --test agent-router/test/*.test.mjs` — **516/516**
- PASS: `npm test --prefix agent-router/pq-auth` — **26/26**
- PASS: `cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked` — **27/27**
- PASS: `cargo test --manifest-path zcash/qpc-zcash-agent/Cargo.toml --locked` — **46/46**

These are the current tree counts. Numbers appearing later in this document (499 JS / 21 NEAR, 516 JS / 24 NEAR, etc.) are historical checkpoints from earlier integration stages and are retained for provenance. They are not the counts of the current commit.

Solana SBF + unsigned LiteSVM evidence is described in LOCAL_EXECUTION.md and the Solana workflow; it is local simulation, not validator consensus. Zcash shielded payment construction remains unimplemented. External professional audit has not been performed. Legacy NEAR state migration is not implemented.

---

# QPC engineering evidence and requirement traceability

## Final offline runtime and security-correction checkpoint

On the original `qpc/near-contract-mvp` branch, remote `ee0aca30a73a479c0a8a3fa25b20dcb351cd818c` was fetched and its tree was exactly identical to local `f883fcf`. A content-preserving merge retained both histories. New runtime and corrective changes below are local; no PR, main-branch change, publication bypass, signing, broadcast or live deployment occurred.

PASS: `RUSTUP_HOME=/tmp/qpc-rustup QPC_SBF_TOOLS=/tmp/qpc-official-sbf/platform-tools FORGE_BIN=/tmp/qpc-foundry/node_modules/@foundry-rs/forge-linux-amd64/bin/forge CARGO_BUILD_JOBS=1 node agent-router/scripts/check-local.mjs` — exit 0, every check passed: **516 JS**, **24 NEAR unit**, **4 actual-WASM VM**, **46 Zcash**, **14 Solana host**, **4 actual-SBF LiteSVM**, **58 Foundry** tests; syntax, formatting, native checks/builds, deterministic demo and whitespace checks also passed. Foundry retains existing lint warnings; this is not a clean static security audit. Additional worker checks passed Solana runtime Clippy with warnings denied and script syntax. Workflow commands ran locally; remote CI itself was not run.

Raw Rust 1.100 NEAR WASM preparation failed before initialization. The required red check was reproduced, and Binaryen 123 memory-copy/fill lowering passed four actual-WASM tests without loosening pinned protocol-79 VM validation. Final integrated compatible WASM SHA-256: `a17f77eb844629c8bc2110a0f2692b20e169467fb705a933b1c2b8c43d226f56`. The compiler-path-dependent build artifact differs from worker evidence hashes; each hash identifies its own actual check, not a reproducible-byte claim. The official platform-tools v1.58 SBF build passed ELF validation; SHA-256: `a2360ac6477908e41434d90efab594b6b486d5e209ae7c83c259d000d7050807`.

Claude Opus 5.5 independently found and rechecked five corrections. M1: preserve valid committed native settlement audit records through restart/expiry, but never rehydrate proof capabilities; removed/mismatched trust still quarantines. M2: pin both NEAR callbacks to 10 Tgas and unused-gas weight zero, asserted in host and actual WASM actions. M3: permanently reserve EVM intent IDs per agent independently of nonce. L1: follow up to five 20-entry Solana approval-history pages with strict cursors/order/duplicate checks. L2: namespace NEAR intent reservations by agent, key receipts/results by commitment, and require explicit agent getters. Recovery regressions failed on all three native chains before the fix; M2/L2 failed three unit tests before the fix; M3 failed two Foundry tests; pagination failed beyond the first history page. All passed after correction. Recheck of corrective patch `99703d8fc5cef81fa4db7202dd5df61860bf45112288dd6b38d106076d5adcca` found all five resolved and no new medium-or-higher defect. Reviewer ran 516 JS, 24 NEAR and 58 Foundry tests; integrating Session, not reviewer, ran the offline VM/SBF suites. This model review is not an external production audit.

Limits remain explicit: NEAR offline external actions/results are mocked, not a full receipt scheduler or native transfer; LiteSVM uses unsigned transactions with signature verification off, not cryptographic authentication or a validator. Its constructor creates an unused ephemeral in-memory airdrop keypair; no wallet key is persisted and no test signs or calls airdrop. Verifier-to-runtime node end-to-end flows, live deployed identities/finality, Zcash shielded settlement, PQ/private transport, production custody/storage, external audit and missing original patch completeness remain unverified or unimplemented. Solana history deeper than 100 entries still fails safe. The changed NEAR layout requires fresh initialization: do not upgrade old state without a separately implemented/reviewed migration.

## Native execution continuation — earlier local check record

The continuation began on local `29c5deaaf49d609bf962c13cd08663735bd8f66a`. Fetch found remote feature HEAD `d5ce72197357b9b79b72298f7256303cd6fe7db4` with identical content but different history; a non-destructive merge preserved both histories. Remote main was `45a8352172e207df6b37c1a305d4c5bb92c01378`, no longer containing the original checkout basis. Publication was refused without pushing or opening a PR. The user explicitly chose to keep `qpc/near-contract-mvp`, so no alternate branch, PR or migration was attempted.

Current implementation adds exact caller-funded NEAR execution/private results/refunds, a separate owner-attested Sepolia/BSC native executor, a real Solana native processor with cross-language unsigned instruction encoding, independent native execution RPC verification, opaque expiring verification handles, independently pinned store trust and atomic durable settlement. See [local execution and trust limits](LOCAL_EXECUTION.md). Generic callback and plain-transfer settlement remain disabled; Zcash shielded settlement remains `NOT_IMPLEMENTED`.

### Passed integrated checks on Reason Cloud Linux

- PASS: `RUSTUP_HOME=/home/runner/.cache/qpc-rustup FORGE_BIN=/tmp/qpc-foundry/node_modules/@foundry-rs/forge-linux-amd64/bin/forge node agent-router/scripts/check-local.mjs` — exit 0. This runs the exact individual commands described below; it performs no signing, broadcast, deployment or real funds movement.
- PASS: `node --test agent-router/test/*.test.mjs` — **499 tests**, 0 failed, 0 skipped at this integrated checkpoint. Native adapter/store tests use mocked native RPC fixtures; they do not prove deployed-executor compatibility.
- PASS: JS source syntax checks; additionally `find agent-router -name '*.mjs' -print0 | xargs -0 -n1 node --check` passed for source/tests/demo/scripts.
- PASS: `node agent-router/demo/simulate.mjs` — 11 expected deterministic simulated outcomes, 0 real transactions/settlements. No visible dashboard change in this continuation.
- PASS: `cargo fmt --manifest-path near/qpc-near-agent-finance/Cargo.toml -- --check`; `CARGO_BUILD_JOBS=1 cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked` — **21 tests**; `cargo check --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked --target wasm32-unknown-unknown`; `cargo build --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked --target wasm32-unknown-unknown --release`.
- PASS: `cargo fmt --manifest-path zcash/qpc-zcash-agent/Cargo.toml -- --check`; `cargo test --manifest-path zcash/qpc-zcash-agent/Cargo.toml --locked` — **46 tests**; `cargo check --manifest-path zcash/qpc-zcash-agent/Cargo.toml --locked`.
- PASS: `cargo fmt --manifest-path solana/qpc-native-executor/Cargo.toml -- --check`; `cargo test --manifest-path solana/qpc-native-executor/Cargo.toml --locked` — **14 host tests**; `cargo check --manifest-path solana/qpc-native-executor/Cargo.toml --locked`; `cargo clippy --manifest-path solana/qpc-native-executor/Cargo.toml --locked --all-targets -- -D warnings`. Rust-generated instruction bytes match all four JS instruction encodings. Host syscall tests explicitly do not simulate bank rollback.
- PASS: Foundry 1.7.1 `forge fmt --check`, `forge test --summary` — **55 tests**, including 19 new native executor tests and amount fuzzing, and `forge build`. Existing timestamp/typecast warnings remain; none was disabled.
- PASS: `git diff --check`, added-line secret-signature scan and sensitive-artifact filename check. These are bounded checks, not a guarantee against every secret encoding.

### Reproductions and recovered check failures

The recovery worker reproduced reauthorization after stripping both replay indexes before adding integrity checks; the new regression rejects it. New crash tests kill a real child before/after rename; they prove process-crash behavior, not power-loss durability. Native proof tests reject forged/cloned handles, stale receipts before and after lock acquisition, wrong authorization/transaction/trust roots, duplicate transaction use, CAS races and asynchronous caller input mutation. Solana upgrade-planted state cannot substitute for an independently discovered canonical owner approval under the pinned immutable program.

Initial checks exposed environment/runner failures and were corrected, not skipped: `forge` was absent (exit 127), recovered using pinned npm `@foundry-rs/forge-linux-amd64@1.7.1`; Zcash `--locked` failed because its lockfile was absent, fixed by including Cargo.lock; the shared toolchain refused WASM installation read-only, so a writable toolchain was used; that toolchain initially lacked rustfmt, installed before rerun; NEAR host `cargo check` failed by SDK design, corrected to the required WASM target. The final integrated runner above passed all its checks.

### Partial and unverified requirements

- PARTIAL: `node agent-router/scripts/testnet-inspect.mjs` — exit 1: actual read-only chain/genesis identity reads succeeded for NEAR testnet, Sepolia, BSC testnet and Solana devnet; Zcash loopback RPC was `NOT_VERIFIED` (no local daemon). The CLI attempts no settlement.
- PASS, narrow live compatibility probes by the adapter worker: NEAR finalized public transaction/receipt schema and `EXPERIMENTAL_receipt`, plus Sepolia/BSC finalized-block EIP-1898 `eth_getCode`/`eth_call`. NEAR numeric FunctionCall gas was observed and incorporated. These do not prove our new executor was deployed or paid anyone.
- BLOCKED/NOT RUN: `cargo build-sbf --manifest-path solana/qpc-native-executor/Cargo.toml` — SBF command unavailable. No SBF/validator integration, actual Solana rollback, new deployed executor identity or live payment is verified.
- NOT_IMPLEMENTED: Zcash shielded payment construction/native settlement; cross-chain atomicity and cross-chain recovery; active PQ/hybrid authentication/private transport; production-grade signer/custody/replicated rollback-resistant storage.
- The original supplied `qpc-phase2-4-final.patch` remains absent from the upload inventory. Its bytes were never fabricated or claimed applied. Independent implementation cannot prove missing-patch completeness.
- An independent model security review is separate from a professional external audit; production funds remain prohibited pending audit and operational/deployment acceptance.

## Historical Phase 4 boundary and validation

This document separates freshly run checks from **historical evidence retained below**. Prior review verdicts, test counts and unavailable historical commit IDs are not attestations for the current working tree. The comprehensive current [security model](../SECURITY.md) defines remaining blockers, not the historical approval language.

Freshly run on Reason Cloud Linux during the Phase 4 documentation pass:

- PASS: `cargo fmt --manifest-path near/qpc-near-agent-finance/Cargo.toml -- --check` (exit 0).
- PASS: `CARGO_BUILD_JOBS=1 cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml` (exit 0): 11 unit tests passed, 0 failed; 0 doc tests. Mock NEAR runtime only, no deployment/native receipt verification.
- PASS: `cargo fmt --manifest-path zcash/qpc-zcash-agent/Cargo.toml -- --check` (exit 0).
- PASS: `CARGO_BUILD_JOBS=1 cargo test --manifest-path zcash/qpc-zcash-agent/Cargo.toml` (exit 0): 46 unit tests passed, 0 failed; 0 doc tests. Parser/wallet/read-only boundary tests, not a shielded payment.
- PASS: `git diff --check` after the documentation edits (exit 0); Python local Markdown-link existence check across README, architecture, security and top-level docs found no missing local targets. These check whitespace/link existence, not semantic correctness.
- Foundry initially unavailable on this worker (`forge fmt --check` and `forge test --summary`: command not found, exit 127). The integrating Session subsequently recovered Foundry via the standard npm binary package `@foundry-rs/forge-linux-amd64@1.7.1`, and reports PASS for its absolute-path `forge fmt --check` and `forge test --summary`: 36 tests, Solc 0.8.24 compilation successful. Those Foundry checks were run by the integrating Session, not re-run by this documentation worker.
- Additional integrating-Session reports: Zcash `cargo check` PASS; `forge build` PASS with existing timestamp/typecast warnings. NEAR host `cargo check --locked` initially failed because near-sdk requires the WASM target; standard target installation failed against the read-only toolchain. A separate writable Rust 1.100 toolchain/target installation succeeded (its subsequent self-update failed read-only, but the installed compiler remained usable). Final `RUSTUP_HOME=/home/runner/.cache/qpc-rustup cargo check --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked --target wasm32-unknown-unknown` PASS and the equivalent `cargo build --locked --target wasm32-unknown-unknown --release` PASS. A WASM build is not deployment, gas certification or native execution proof.

**Security correction in the current integration:** callback-true results are not production native evidence. Generic settlement evidence remains `NOT_VERIFIED`; `FileIntentStore.confirm` cannot confirm it, and `settle` rejects even legacy confirmation records. Native mock evidence is only `MOCK_VALIDATED_NOT_SETTLED`; `verifyNearToZcashRoute` cannot turn provider success into shielded settlement and returns `NOT_IMPLEMENTED`. The integrating Session owns those source changes and regression checks; their exact final results must accompany its delivery. No signing, broadcast, deployment or production behavior was exercised by this documentation pass.

Read-only live endpoint observations and unsigned serialization checks, if reported by the native adapter pass, establish only the exact compatibility checks run, not signer authorization, consensus finality or payment. A deterministic dashboard is a labeled simulation, not a chain receipt. No PQ/hybrid signature, ML-KEM, ML-DSA, SLH-DSA, private agent transport or production settlement verifier is active.

## Final integrated Phase 2–4 check record

Actual starting feature-branch HEAD was `3023ab50dac8d255e284adf2b485c7671734b6c8`, clean and identical to origin after fetch. Supplied baseline `56107a3e0659845f661902ef1e075efa75710f09` could not be resolved from origin (`not our ref`), so its ancestry is not verified. The supplied `qpc-phase2-4-final.patch` was absent; these changes implement the explicit requirements independently, not an attestation that the missing patch was applied.

- PASS: `node --test agent-router/test/*.test.mjs`: **216 tests, 0 failures** on the final integrated source. Dedicated testnet adapter coverage: 100 tests.
- PASS: `find agent-router -name '*.mjs' -print0 | xargs -0 -n1 node --check` (includes demo and inspection CLI).
- PASS: `node agent-router/demo/simulate.mjs`: **11 expected outcomes**, 9 rejected and 2 accepted only in explicit simulation, 0 real transactions/settlements. Its tests compare byte-identical generated HTML/JSON. Desktop dashboard and expanded forged-proof/settlement rejection were browser-inspected; generated source is retained in `agent-router/demo/`.
- RED then GREEN: `node --test agent-router/test/settlement-boundary.test.mjs`: two regression tests failed before fixing callback-only verification/confirmation, then passed after; legacy persisted confirmation records cannot settle. `node --test agent-router/test/zcash-route.test.mjs` similarly failed its new no-shielded-settlement assertion before the fix and passed afterward.
- RED then GREEN: the adapter worker reproduced 14 EVM blob/delegation regression failures, then passed all 100 adapter tests after restricting supported transaction types and rejecting blob/delegation fields. These are synthetic RPC responses, not real consensus/reorg experiments.
- PASS: final integration reran `cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked` (11 tests), `cargo test --manifest-path zcash/qpc-zcash-agent/Cargo.toml` (46 tests), and both `cargo fmt` checks above. No Rust source changed.
- PASS: `/home/runner/.cache/qpc-forge/node_modules/@foundry-rs/forge-linux-amd64/bin/forge fmt --check`, the same binary's `forge test --summary` (36 tests), and `forge build`. Foundry 1.7.1 was installed outside the repository from pinned npm `@foundry-rs/forge-linux-amd64@1.7.1` after the initial missing executable.
- PARTIAL: `node agent-router/scripts/testnet-inspect.mjs` exits **1**, correctly reporting four successful read-only chain/genesis identity observations (NEAR testnet, Sepolia, BSC testnet, Solana devnet), and Zcash `http://127.0.0.1:18232` as `NOT_VERIFIED` (`fetch failed`, no configured local daemon). This proves endpoint identity reads only, not transactions or settlement.
- NOT IMPLEMENTED: native settlement tied to on-chain authorization commitment, nonce/replay, expiry/revocation and policy version; Zcash shielded preparation/receipt verification; transaction wire serialization/signing/submission; active PQ/private-agent transport. A matching native transfer observation is not QPC authorization proof.
- BLOCKED: `git push origin HEAD:refs/heads/qpc/near-contract-mvp` was refused by the environment, which only permits publication through PR creation. The user prohibits PRs, so no alternate credential/route or PR was used. Remote feature HEAD remains `3023ab50dac8d255e284adf2b485c7671734b6c8`; `main` remains `45a8352172e207df6b37c1a305d4c5bb92c01378`. Final local commit identities and downloadable patch are in the delivery message.

No release, deployment, transaction signing, broadcast, transfer, PR or merge occurred. The exact patch's absence and refused push prevent claiming full requested delivery.

## Historical scope and provenance (not reverified current-head evidence)

Baseline: `14c6c231a838` on exact branch `qpc/near-contract-mvp`. Main was not modified. Local verification ran on Reason Cloud Linux, Node 22.23.5, Foundry 1.7.1 and Solc 0.8.24. No deployment, wallet signing, broadcast, transfers, merging or submission occurred. There is no production verification claim.

First verified milestone: `41da2b1` (code-less token rejection and non-vacuous stateful spending). PR creation was attempted after that milestone but the supported publication tool refused because `main` does not contain the Session's starting commit. Nothing was pushed and no PR/CI URL exists. This is a publication-base ancestry limitation, not proof of missing GitHub write permission. An exact patch is the fallback deliverable; its final identity/digest accompany the report.

## Historical reproduction and regression evidence

- Baseline `node --check agent-router/src/cross-l1-envelope.mjs`: FAIL, escaped backtick at line 15 (`SyntaxError: Invalid or unexpected token`). Baseline `node --test agent-router/test/*.test.mjs`: FAIL, envelope test escaped template at line 138; six existing route tests passed. After correction both parse and full tests PASS.
- `forge test --match-test testRejectsEOATokenWithoutConsumingCap -vv`: RED, `EOA accepted as successful token transfer`; same test GREEN after requiring deployed token code. Failed calls preserve cap and nonce.
- `forge test --match-test testStatefulExecutionIsReachable -vv`: RED, `campaign never executes approved calls`; GREEN after configuring exact calldata permission, funding and explicitly targeting the handler in a real invariant campaign.
- Strengthened `forge test --match-test testReentrancy -vv`: RED, outer call failed when requiring the Reentrancy selector; GREEN after making the malicious destination an authenticated agent so the test actually reaches the lock rather than rejecting earlier. Reentrancy is blocked by the lock; the malicious callback cannot re-enter.
- Additional Foundry security tests cover pause, expiry, recipient allowlist, calldata-hash binding, allowance-selector denial, and native/token cap accounting. Invariant and stateful tests exercise spending under concurrent agent activity.
- All historical red-green pairs were resolved before the counts above. No unresolved failing test is claimed.

This document continues to distinguish historical evidence from current-tree verification. Production funds remain prohibited until external professional audit, deployment verification, and the remaining blockers listed in SECURITY.md are addressed.
