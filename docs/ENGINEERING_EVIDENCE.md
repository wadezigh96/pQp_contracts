# QPC engineering evidence — owner and replay hardening

**Branch:** `reason/deliver-qpc-hardening-dr-d997d6a3`
**Starting revision:** `b1c758117d131b4974bada543f49b378149f8127`
**Scope:** local implementation and PR review only. No deployment, public transaction, key rotation, real-funds operation, or external professional audit was performed.

## Implemented security corrections

- NEAR initialization now requires (1) empty state, (2) `predecessor_account_id` equal to the supplied owner, and (3) the pinned owner `wkams.testnet`. A regression test supplies the correct owner argument from `attacker.testnet` and requires rejection before state creation. Contract storage ownership remains distinct from account-level access-key and deployment authority.
- The JavaScript replay-domain key is now versioned `V2` and binds `source.adapterId` in addition to replay authority/scope, network, native chain, source account, and agent identity. A regression test checks that changing only the source adapter changes the replay domain. The canonical envelope serialization and authorization commitment are unchanged. Existing durable replay stores using the former V1 key derivation require an explicitly designed, atomic migration before any upgrade; no replay-store migration is implemented here.

The shared fixture `docs/fixtures/cross-l1-v1.json` still produces commitment `c72c9a07dfd9ce8aecbd21c9c6bf858e5f089072a9ec84bafb03aadada653333`; a fresh local computation after the replay-key change matched this literal value. **Release gate:** do not upgrade an existing durable store to V2 without a tested atomic rekey migration that preserves all consumed claims. Freeze V1 writers during migration and prevent mixed V1/V2 authorities from accepting the same replay claim. Never delete/reset a V1 store to bypass the V2 consistency rejection. No store migrator is implemented.

## NEAR test-count reconciliation

The CI run on the starting revision (`b1c758117d131b4974bada543f49b378149f8127`) executed **31 Rust unit tests**, not 33. Repository history shows 27 tests before owner hardening; commit `0166383` added four owner tests, giving 31. The “29 baseline / 33 expected” wording was unsupported by source history and test discovery. This change adds one predecessor-authorization regression test, so the current source declares **32** NEAR tests; the result for this revision must be taken from its post-push CI run, not inferred from source count.

## Verification evidence

### Local, on the modified working tree

- `node --test agent-router/test/*.test.mjs`: **517 passed, 0 failed** (fresh complete run, exit code 0).
- `node --test agent-router/test/cross-l1-envelope.test.mjs`: **60 passed, 0 failed**.
- `npm test --prefix agent-router/pq-auth`: **26 passed, 0 failed** after installing the lockfile-pinned dependencies with `npm ci --prefix agent-router/pq-auth --ignore-scripts --no-audit --no-fund`.
- `git diff --check`: passed on the current complete diff; rerun before commit.
- NEAR test source discovery: **32** `#[test]` declarations. NEAR formatting and Rust tests for the modified revision have not been run locally because `cargo`, `rustc`, and `rustfmt` are absent. The current workflow run on the modified revision is the authority for Rust results.
- `node --test agent-router/test/intent-store-recovery.test.mjs`: **10 passed, 0 failed**. The new compatibility regression verifies a V1-derived persisted replay index is rejected under V2 without rewriting/resetting the file.

### Existing remote CI on the starting revision only

- [NEAR run 43541003900](https://github.com/wadezigh96/pQp_contracts/actions/runs/43541003900): **31 Rust unit tests passed** and a separate **5 offline-VM tests passed**. Formatting and compatible WASM/offline-VM checks passed in that workflow. These results apply to the starting revision, not the code changes described above.
- [Zcash adapter run 43541003951](https://github.com/wadezigh96/pQp_contracts/actions/runs/43541003951): **46 adapter unit tests passed** on the starting revision. This workflow does not run Zcash regtest.
- The starting PR revision reported 11/11 checks successful. That is historical evidence, not a result for the modified revision.

## Network and settlement boundaries

- **Actual NEAR Sandbox:** BLOCKED / NOT RUN. The execution environment has no Rust toolchain or `near-sandbox`. The CI offline VM executes the built WASM with mocked host context; it is not a nearcore Sandbox, account deployment, signed transaction, or live receipt graph.
- **Zcash regtest:** BLOCKED / NOT RUN. `zcashd`, `zebrad`, `lightwalletd`, and Docker are absent. More importantly, this repository does not implement Zcash transaction construction/signing/broadcast, wallet synchronization, or independent receipt/confirmation verification. Installing node binaries alone would not create the missing payment flow.
- **NEAR → Zcash / Zcash → NEAR transactions:** NOT RUN. There are no transaction IDs or settlement results to report. The router explicitly returns `NOT_IMPLEMENTED` for native shielded Zcash settlement.
- Zcash adapter tests validate addresses, policy/replay boundaries, wallet database behavior, and read-only probes; they are not regtest or Testnet transactions.

## Account-level authority

`funzest6203.testnet` is the intended deployment target only. No deployment occurred. NEAR account access keys and deployment authority are outside contract storage ownership; a WASM method cannot revoke or constrain FullAccess keys. No account credentials or key material were requested, stored, or transmitted.

## Claims not supported

- No production readiness, live payment, cross-chain atomic settlement, or post-quantum security claim.
- No claim of a professional external audit.
- Unit tests, offline VM tests, NEAR Sandbox, Zcash regtest, and Zcash Testnet transactions are distinct and are not interchangeable.
