# Offline NEAR WASM boundary checks

This crate executes the **built contract WASM**, not host calls to `QPCNear`, through pinned `near-vm-runner 0.31.1` with its supported Wasmtime backend and protocol **79** configuration. The VM prepares, instruments, links NEAR host functions and meters the actual WASM ABI entrypoints. This deliberately pinned historical protocol is **not a claim about today's NEAR testnet/mainnet configuration**.

## Repeatable local check

Prerequisites: Rust with the `wasm32-unknown-unknown` target, Cargo, Node/npm, and a native C build toolchain for the VM dependencies. Verified with Rust **1.100.0**, Node **22.23.5**, Binaryen **123.0.0**. Cargo and npm dependency versions are locked. Initial dependency installation downloads packages; the tests themselves use no RPC, network, keys or chain accounts.

From the repository root:

```bash
npm ci --prefix near/offline-vm --ignore-scripts --no-audit --no-fund
CARGO_BUILD_JOBS=1 bash near/offline-vm/check.sh
```

The script builds the contract for `wasm32-unknown-unknown` with `-C target-cpu=mvp`, then applies pinned Binaryen's `--llvm-memory-copy-fill-lowering`. It preserves the raw build and writes the tested artifact next to it as `qpc_near_agent_finance.compatible.wasm`. Do not confuse the raw, merely compiled artifact with this VM-checked artifact. `WASM_OPT` can select an existing executable but its reported version must be 123. `QPC_NEAR_WASM` can select an explicit artifact when invoking this crate's Cargo tests directly.

To also reproduce and check the raw-artifact rejection before the passing run:

```bash
QPC_REPRODUCE_RAW_FAILURE=1 CARGO_BUILD_JOBS=1 bash near/offline-vm/check.sh
```

That mode requires the raw check to fail specifically with `PrepareError(Deserialization)`, captures its log in the contract's ignored `target/raw-vm-rejection.log` (a measured run is also preserved in `evidence/raw-vm-rejection.txt`), then runs the complete suite against the lowered artifact. If a future toolchain makes the raw artifact compatible, this diagnostic mode fails and asks for reassessment; it does not convert arbitrary failures into success.

## What the five tests establish

- A checked root header (`QPCNEAR\0LAYOUT\0\0`, layout version 1) is validated by generated WASM ABI wrappers. Exact prior unversioned roots, corrupt magic, unknown versions and truncated headers reject both views and mutations without storage/action changes; initialization cannot overwrite them. A restored supported root remains readable. This is incompatible-state rejection, **not legacy migration or recovery**.
- Fresh initialization, owner profile/recipient configuration, native predecessor authorization and durable storage across separate WASM invocations. Authorization alone schedules no transfer and cannot later be repurposed to execute the consumed intent or claim external settlement.
- Exact-deposit execution schedules a 1,000-yoctoNEAR transfer to the bound recipient and a private, zero-deposit callback whose receipt depends on that exact transfer. The callback arguments bind the envelope commitment and intent. Pending/success results are readable through actual read-only WASM getters.
- Actual WASM callback rejects a non-contract predecessor; injected successful transfer outcome produces `EXECUTED`; duplicate callback, execution replay and success refund claims reject. The successful transfer callback runs with the actual scheduled **10 Tgas** allowance.
- Injected failed transfer result preserves `FAILED` separately from refund recovery. Refund is original-agent-only, schedules the exact amount back to that agent, rejects concurrent claims and forged callbacks, permits retry only after an injected failed refund result, rejects the stale prior attempt and finishes `REFUNDED` only after a successful current callback. The successful refund callback also runs within **10 Tgas**.
- A **39 Tgas** execution allowance rejects at the contract's minimum-gas gate. Expiry and agent revocation reject without producing authorization/execution records.

All accepted ABI calls report nonzero metered gas and stay within supplied gas. These are pinned-VM gas units, **not actual protocol tokens burnt or verification of the all-in fee cap**; the result continues to report `protocolGasFee: "NOT_VERIFIED"` and `feeBudgetEnforced: false`.

## Boundaries that remain unproved

`MockedExternal` supplies an in-memory trie and records host actions. The harness deliberately injects `PromiseResult` values and predecessor context into the actual WASM callback; it does **not execute the scheduled transfer**, generate native receipts, maintain multi-account balances, run a nearcore receipt scheduler, enforce keys/signatures, or prove consensus/finality/current-network compatibility. The harness snapshots/reverts its mock trie on VM abort because near-vm-runner delegates transaction rollback to its caller. That is an explicit harness assumption, not a tested nearcore rollback implementation. Account balance and initial storage usage are synthetic; no funds move. These checks therefore close the WASM-execution gap but **not** the full-runtime/balance/receipt-graph gap or cross-chain settlement gap.

No node, signing, broadcast, deployment, migration, funds operation or contract state change outside this process is part of this check. Tests initialize local state; the layout regression deliberately substitutes incompatible root bytes into that mock trie and verifies rejection without replacing the original maps. No historical deployed state is migrated.

## Reproduced compatibility failure and fix

On this checkout the original default Rust 1.100 release WASM had SHA-256 `bc6dbd86f6ba288e105da1c72a8e12a8e004223ff034ecdd3095cbcd716ce191`; `new` failed before contract execution with:

```text
new: Some(CompilationError(PrepareError(Deserialization)))
test result: FAILED. 0 passed; 3 failed
```

Rebuilding with `target-cpu=mvp` alone still failed: raw SHA-256 `6ea378e90e3cedef1c537bf40975bf55d9aafd9492cdb0d92487b9d6d5b1329e`. Rust's precompiled standard library still contained bulk-memory instructions. Merely re-encoding this module with Binaryen also failed, ruling out re-encoding alone as the fix. Lowering **only** `memory.copy`/`memory.fill` with Binaryen 123 made the same pinned runner accept and execute the module: compatible SHA-256 `47ecae1fa285d4c6a0966a65d06d8874c357992f467755436e6fbb29c8cc0867`, **4 passed, 0 failed**. Hashes identify this measured build, not guaranteed byte-for-byte output on every toolchain or source revision. No permissive VM feature switches were used to waive the protocol's filtering.

The runner's `near_vm` backend initially failed to link under Rust 1.100 with upstream `undefined symbol: __rust_probestack`; selecting its officially supported `wasmtime_vm` backend avoided that toolchain incompatibility while retaining NEAR's preparation/feature filtering. There is an upstream future-incompatibility warning for transitive `wasmparser 0.78.2`; it does not fail the tests.
