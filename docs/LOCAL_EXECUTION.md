# Authorization-bound local execution

## Scope and safety boundary

These are **unaudited local execution prototypes**, not a bridge, a deployed service, a production audit, or an active post-quantum system. No test run signs or sends a live transaction. A native execution result is not cross-chain settlement. Generic caller-proof settlement remains disabled.

Use the existing [canonical envelope specification](CANONICAL_SPEC.md) and [security model](../SECURITY.md). Plain native transfers cannot prove the envelope's policy, nonce, expiry, identity or revocation state. A matching amount/recipient is insufficient.

## NEAR: native caller-funded same-chain payment

`execute_near_envelope` combines the existing owner-profile authorization gate with a single native NEAR transfer and a private result callback. The supported local route is deliberately narrow: `near:testnet` on both sides, the same QPC contract as source/destination adapter, `near:native` with 24 decimals, exact identity amount mapping, native payment only, no slippage, and an exact recipient commitment. The attached deposit must equal the payment amount. This is not an escrow-spending or cross-chain method.

All policy/profile/time/nonce/intent checks occur before transfer scheduling. Replay remains consumed if the later asynchronous transfer fails. Only the contract's private callback, reading the native promise result, may record `EXECUTED` or `FAILED`; a caller-supplied boolean cannot do so. A pending or failed result is never payment success. Revocation prevents new authorization; it cannot undo an already scheduled transfer.

A failed transfer's returned funds require the separate, agent-authorized refund state machine. Refund attempts cannot repeat a pending or completed refund, cannot re-open the payment nonce, and do not change a failed payment to successful. An ambiguous callback requires reconciliation rather than another payment or refund. These are host mock-runtime tests, not proof of a deployed NEAR receipt graph. The contract's expanded state layout is for fresh local initialization; no migration of deployed state is claimed.

The envelope's `maxFeeAtomic` retains its **all-in native fee budget** meaning. The NEAR host API does not expose the caller's gas price to this contract. Exact deposit prevents an extra contract service fee, but does not enforce protocol gas costs. Result metadata makes that limit explicit. Independent inspection must account for all transaction/receipt `tokens_burnt` before claiming fee conformance. Do not reinterpret this budget as service-only or treat an `EXECUTED` record as final settlement.

## Sepolia and BSC testnet: exact owner-attested intent

`QPCNativeTestnetExecutor` is separate from the pre-existing `QPCAgentFinance` policy executor. It accepts only chain IDs 11155111 and 97. The owner approves one immutable association between a canonical envelope commitment and typed execution parameters; the contract does **not** parse or recompute canonical JSON. Owner attestation is a trust root, not a cryptographic proof that arbitrary bytes describe the claimed parameters.

The approved record binds intent/policy hashes, policy commitment, agent, recipient, native asset/18 decimals, amount, chain, executor address, nonce, policy version, revocation epoch, issue/not-before/expiry times and fee budget. Approval permanently reserves the agent nonce. Agent execution takes only the commitment: recipient/amount cannot be overridden. It requires exact caller-funded value, current policy/epoch, supported chain, time window, unused successful intent and a non-reentrancy lock. A failed EVM call reverts payment and execution state atomically, allowing only the same immutable approval to be retried while valid. Successful execution cannot repeat.

`prepareEvmOwnerApprovedExecution` generates frozen unsigned data for configuration/approval/execution. It never authenticates an owner, supplies a signer or serializes/broadcasts a signed transaction. The identifier hashes are SHA-256 over `QPC-EVM-INTENT-ID-V1\0` plus the UTF-8 intent ID and `QPC-EVM-POLICY-ID-V1\0` plus the policy ID. The full commitment uses the existing canonical envelope domain.

The executor emits `EXECUTED`, not `SETTLED`. It records `maxFee` but cannot enforce all caller transaction/validator fee mechanics; independent RPC verification must inspect actual fees, transaction contents, approved state, canonical block identity and the chain-specific finality policy. An event from an arbitrary contract is not evidence: operator-pinned address, owner and deployed runtime identity are required.

## Solana: owner-attested native program and wire preparation

`solana/qpc-native-executor` is a real `solana-program` processor, with fixed-width instruction/state encoding, owner/agent policy PDA and immutable per-nonce intent PDA. Initialization/approval allocate and assign PDA accounts through signed SystemProgram CPI, including already-prefunded-address handling. This is program-internal PDA authority, not a test signing or broadcasting a live transaction. Owner attestation binds the canonical commitment to native lamports/9 decimals, recipient, amount, policy, nonce, epoch and time window. Runtime execution requires the exact native signer/accounts/program and one agent-funded SystemProgram transfer; replay state is set before CPI. A failed CPI's transaction rollback is a Solana runtime property, not proven by host stubs.

The unsigned JS preparation uses the actual Rust instruction encoding and a cross-language fixture. It emits no signatures. Caller-supplied PDA addresses are explicitly not independently validated by the preparation; the runtime program derives and checks them. Operator genesis metadata and `max_fee` remain owner-attested values: the on-chain program cannot independently discover the cluster genesis or enforce the transaction's protocol fee. Native RPC verification must separately prove these values rather than treat stored metadata as proof.

Host processor tests and Clippy are checked. SBF compilation, bank/validator execution and actual runtime rollback are unverified while the SBF toolchain is unavailable. No deployed Solana executor is claimed. Missing historical state or code-at-execution evidence must fail closed; `minContextSlot` is a lower context bound, not a historical account-state API.

## Native verification receipt and durable settlement

The dedicated authorization-bearing execution inspectors do not accept a caller evidence object or verifier callback. They read RPC independently, bind exact transaction input/value/signers and authorization-bearing executor state, verify successful native outcomes, actual fees, historical operator-pinned deployed code/owner, canonical inclusion and finality. NEAR additionally reads native receipt bodies and validates transfer/callback linkage. EVM validates immutable approved parameters, policy epoch, event provenance and transaction gas/type restrictions. Solana requires a finalized exact execution and sole transfer CPI, correct balances/fees, matching policy and executed intent records, immutable UpgradeableLoader ProgramData with pinned code strictly predating execution, and an independently found canonical owner-signed approval after that deployment and before execution. Bounded/pruned approval history, mutable code, same-slot deployment or incomplete native evidence fail closed. `minContextSlot` is never treated as historical state. Plain transfers, Zcash and unsupported execution programs still cannot settle.

`verifyNearExecutionSettlement`, `verifyEvmExecutionSettlement` and `verifySolanaExecutionSettlement` can produce an opaque process-local verification handle. A cloned object, metadata JSON, boolean or old persisted confirmation is not that handle. `readVerifiedNativeSettlement` recognizes only module-owned handles and checks 60-second wall-clock freshness (including rejecting backward time). The private creation timestamp is not supplied by the caller. This is verification under explicit RPC-provider, code-pin and upgrade-key custody trust, not a light-client consensus proof.

Configure `FileIntentStore` with independent operator-owned `nativeTrust` identities (`chain`, `networkId`, `nativeChainId`, `contractAddress`, `codeHash`, `owner`). The default empty trust list rejects native settlement. `settleNative(id,{verification: receipt,nowUnix,expectedRevision})` rechecks the opaque handle and trust roots under the filesystem lock, matches the exact commitment/network/known transaction, rejects a transaction reserved by another intent and uses compare-and-swap to atomically record `CONFIRMED` then `SETTLED`. It never sends funds. Recovery or a process restart requires fresh native verification, not trusting saved proof metadata. The generic `confirm` and `settle` APIs remain fail-closed.

All executor integration success/failure paths in this change use deterministic mocked RPC responses, NEAR mock runtime, Foundry local EVM or Solana host syscall stubs. A separate live read-only NEAR schema probe confirms RPC receipt-method availability only. No newly deployed executor, live payment, production deployment identity or native-validator end-to-end flow has been exercised.

## Crash recovery and local durability

`FileIntentStore.recover({nowUnix})` validates commitments, histories and both replay indexes before atomically moving ambiguous submission and legacy confirmation states into `RECONCILING`. It preserves transaction IDs, nonce/intent reservations and revisions. Missing/orphaned/mismatched indexes reject; they are never regenerated by discarding old records. A repeated recovery is a no-op. It never steals a lock, resends a transaction, accepts a caller verifier or marks payment settled.

Regression tests kill a real child process before/after rename and reopen the store, exercise cancellation during authorization, reject forged evidence and repeated settlement, and preserve state if recovery fails. Process-crash evidence does not establish power-loss, hostile-host, rollback-resistant or multi-host durability. Production still requires transactional storage and reconciliation against native state.

## Reproducible checks

Run from the repository root. `node agent-router/scripts/check-local.mjs` runs the combined local checks; set `FORGE_BIN` if Foundry is installed outside PATH. Individual commands:

```bash
node --test agent-router/test/*.test.mjs
find agent-router -name '*.mjs' -print0 | xargs -0 -n1 node --check
node agent-router/demo/simulate.mjs
cargo fmt --manifest-path near/qpc-near-agent-finance/Cargo.toml -- --check
CARGO_BUILD_JOBS=1 cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked
cargo build --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked --target wasm32-unknown-unknown --release
cargo fmt --manifest-path zcash/qpc-zcash-agent/Cargo.toml -- --check
CARGO_BUILD_JOBS=1 cargo test --manifest-path zcash/qpc-zcash-agent/Cargo.toml --locked
cargo check --manifest-path zcash/qpc-zcash-agent/Cargo.toml --locked
cargo fmt --manifest-path solana/qpc-native-executor/Cargo.toml -- --check
cargo test --manifest-path solana/qpc-native-executor/Cargo.toml --locked
cargo clippy --manifest-path solana/qpc-native-executor/Cargo.toml --locked --all-targets -- -D warnings
forge fmt --check
forge test --summary
forge build
git diff --check
```

Use an installed writable Rust toolchain with the WASM target for the NEAR build. Foundry must be installed; do not omit failed checks from the evidence record. The read-only identity CLI is separate: `node agent-router/scripts/testnet-inspect.mjs`. Its failure exit when a Zcash node is unavailable is **not** a passing five-chain connectivity test.

## Remaining external acceptance requirements

No local test proves a live payment, native deployed program identity, cross-chain atomicity, Zcash shielded settlement, real production fee/finality behavior, custody security, or active PQ authentication. Solana host-program checks must be distinguished from SBF compilation and validator execution. An independent code review is narrower than a professional external audit. Do not use funds before the latter, verified deployment/configuration, operational controls and separately authorized native integration tests.

The original `qpc-phase2-4-final.patch` was not present in the available upload inventory; these independently specified changes cannot establish completeness against missing bytes. Preserve that gap rather than fabricate a patch.
