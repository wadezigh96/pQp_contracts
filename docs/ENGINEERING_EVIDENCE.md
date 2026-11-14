# QPC engineering evidence — owner and replay hardening

**Branch:** `reason/deliver-qpc-hardening-dr-d997d6a3`
**Starting revision:** `b1c758117d131b4974bada543f49b378149f8127`
**Hardening code revision:** `db9f7178f593a10096a8d40f61fe60d77ae583ca`
**Final pushed head with completed CI:** `5a523cbf9f8d46152bdcaaf9004ebd185829ffbd`
**Scope:** local implementation and PR review only. No deployment, public transaction, key rotation, real-funds operation, or external professional audit was performed.

## Implemented security corrections

- NEAR initialization now requires (1) empty state, (2) `predecessor_account_id` equal to the supplied owner, and (3) the pinned owner `wkams.testnet`. A regression test supplies the correct owner argument from `attacker.testnet` and requires rejection before state creation. Contract storage ownership remains distinct from account-level access-key and deployment authority.
- The JavaScript replay-domain key is now versioned `V2` and binds `source.adapterId` in addition to replay authority/scope, network, native chain, source account, and agent identity. A regression test checks that changing only the source adapter changes the replay domain. The canonical envelope serialization and authorization commitment are unchanged. Existing durable replay stores using the former V1 key derivation require an explicitly designed, atomic migration before any upgrade; no replay-store migration is implemented here.

The shared fixture `docs/fixtures/cross-l1-v1.json` still produces commitment `c72c9a07dfd9ce8aecbd21c9c6bf858e5f089072a9ec84bafb03aadada653333`; fresh local computation on the code revision matched this literal value. **Release gate:** do not upgrade an existing durable store to V2 without a tested atomic rekey migration that preserves all consumed claims. Freeze V1 writers during migration and prevent mixed V1/V2 authorities from accepting the same replay claim. Never delete/reset a V1 store to bypass the V2 consistency rejection. No store migrator is implemented.

## NEAR test-count reconciliation

The CI run on the starting revision (`b1c758117d131b4974bada543f49b378149f8127`) executed **31 Rust unit tests**, not 33. Repository history shows 27 tests before owner hardening; commit `0166383` added four owner tests, giving 31. The “29 baseline / 33 expected” wording was unsupported by source history and test discovery. This change adds one predecessor-authorization regression test: source declares 32 tests, and current code-revision CI executed **32, with 32 passed and 0 failed**.

## Verification evidence

### Local checks on the code revision before push

- `node --test agent-router/test/*.test.mjs`: **517 passed, 0 failed**.
- `node --test agent-router/test/cross-l1-envelope.test.mjs`: **60 passed, 0 failed**.
- `node --test agent-router/test/intent-store-recovery.test.mjs`: **10 passed, 0 failed**. The compatibility regression verifies a V1-derived persisted replay index is rejected under V2 without rewriting/resetting the file.
- `npm test --prefix agent-router/pq-auth`: **26 passed, 0 failed** after installing lockfile-pinned dependencies with `npm ci --prefix agent-router/pq-auth --ignore-scripts --no-audit --no-fund`.
- `git diff origin/reason/deliver-qpc-hardening-dr-d997d6a3..HEAD --check`: passed before pushing code revision `db9f717`.
- `bash -n scripts/demo-near-zcash.sh`, a fresh execution of the preflight script from an archive of the pushed commit, and JSON assertions passed. The preflight reported **BLOCKED**, every requested scenario blocked, and no transaction IDs.
- A fresh local computation of the fixture authorization commitment matched the canonical literal above.
- Local `cargo`, `rustc`, and `rustfmt` are absent, so local NEAR formatting and Rust tests could not be run. Current Rust/VM results below come from GitHub Actions.

### Current final-head CI (`5a523cbf9f8d46152bdcaaf9004ebd185829ffbd`; implementation unchanged since `db9f717`)

- [NEAR contract run 43546720154](https://github.com/wadezigh96/pQp_contracts/actions/runs/43546720154): **32 Rust unit tests passed, 0 failed**. The same workflow separately built compatible WASM and ran **5 offline-VM tests, 5 passed, 0 failed**. This is mocked-host/offline-VM execution, not NEAR Sandbox.
- [Zcash adapter run 43546720130](https://github.com/wadezigh96/pQp_contracts/actions/runs/43546720130): **46 adapter unit tests passed, 0 failed**. This does not run Zcash regtest.
- [PR #1](https://github.com/wadezigh96/pQp_contracts/pull/1): **10/10 listed status checks successful** on the final head, with 0 failing and 0 pending.

### Historical CI on the starting revision only

- [NEAR run 43541003900](https://github.com/wadezigh96/pQp_contracts/actions/runs/43541003900): 31 Rust unit tests and 5 separate offline-VM tests passed. These results apply to the starting revision, not the hardening changes.
- [Zcash adapter run 43541003951](https://github.com/wadezigh96/pQp_contracts/actions/runs/43541003951): 46 adapter unit tests passed on the starting revision.

## Network and settlement boundaries

- **Actual NEAR Sandbox:** BLOCKED / NOT RUN. The execution environment has no Rust toolchain or `near-sandbox`. The CI offline VM executes built WASM with mocked host context; it is not a nearcore Sandbox, account deployment, signed transaction, or live receipt graph.
- **Zcash regtest:** BLOCKED / NOT RUN. `zcashd`, `zebrad`, `lightwalletd`, and Docker are absent. More importantly, this repository does not implement Zcash transaction construction/signing/broadcast, wallet synchronization, or independent receipt/confirmation verification. Installing node binaries alone would not create the missing payment flow.
- **NEAR → Zcash / Zcash → NEAR transactions:** NOT RUN. There are no transaction IDs or settlement results to report. The router explicitly returns `NOT_IMPLEMENTED` for native shielded Zcash settlement.
- Zcash adapter tests validate addresses, policy/replay boundaries, wallet database behavior, and read-only probes; they are not regtest or Testnet transactions.

## Account-level authority

`funzest6203.testnet` is the intended deployment target only. No deployment occurred. NEAR account access keys and deployment authority are outside contract storage ownership; a WASM method cannot revoke or constrain FullAccess keys. No account credentials or key material were requested, stored, or transmitted.

## Claims not supported

- No production readiness, live payment, cross-chain atomic settlement, or post-quantum security claim.
- No claim of a professional external audit.
- Unit tests, offline VM tests, NEAR Sandbox, Zcash regtest, and Zcash Testnet transactions are distinct and are not interchangeable.
