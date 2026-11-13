# Local-only verification, operations and demo

The supported verification workflow is local/unsigned and read-only. It must not deploy, use a wallet to sign, broadcast, transfer funds or submit a hackathon entry. Nothing in this guide is evidence of a deployed product. A demo may simulate state transitions, but it must label them as simulation and keep production settlement disabled.

## Reproducible unsigned demo

Run `node --test agent-router/test/*.test.mjs`. The public API tests construct canonical envelopes, compare a literal cross-language vector, authenticate only through explicitly supplied test boundaries, prepare unsigned native requests and reject adversarial substitutions. The local lifecycle tests use temporary directories, concurrent store instances and restart reads. Mock native evidence returns `MOCK_VALIDATED_NOT_SETTLED`; generic evidence returns `NOT_VERIFIED` even if an injected callback returns true. `FileIntentStore.confirm` cannot promote generic evidence and `settle` rejects, including historical confirmations. `verifyNearToZcashRoute` returns `NOT_IMPLEMENTED` instead of treating provider success as shielded settlement. No wallet is used by these tests; the local unit test command requires no live network.

Run `cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml` for native predecessor authorization and policy/replay enforcement in a mocked NEAR runtime. A mocked runtime is not a deployed NEAR node. Run `cargo build --manifest-path near/qpc-near-agent-finance/Cargo.toml --target wasm32-unknown-unknown --release` to produce a local WASM artifact only.

Run `forge fmt --check`, `forge build --sizes` and `forge test --summary` for Solidity unit, security and stateful/invariant tests. These use local Foundry EVM execution, not a network broadcast.

## Deployment boundary (not performed)

Before any future release: obtain separate deployment authority; review incompatible NEAR state-layout/API changes and migration; pin native network/profile/contract identities; use audited native signer and proof providers isolated from agents; provision production atomic storage with backup/restore testing; measure gas/storage and resource limits; verify deployed WASM/bytecode and constructor/owner; validate canonical vectors across released clients; independently audit crypto and economic behavior. Local build success does not establish deployability, production performance or consensus verification.

## Failure handling

The prototype does not broadcast. In any future independently reviewed executor, do not automatically resubmit after an uncertain broadcast. Persist the intent and expected request digest and atomically reserve submission before external work. Treat timeout, partial execution, conflicting inclusion or lost finality as reconciliation. Cancellation and expiry before submission never free consumed nonce/intent keys. Reconciliation records are evidence references, not refunds; no refund/compensation protocol is implemented. Never infer that a provider callback, transaction hash, local record or success-shaped object proves external settlement.

## Privacy and quantum posture

Public calldata, contract state, predecessor/account links, amounts, timestamps, policy profiles, transaction IDs and recipient commitments remain observable. A deterministic recipient commitment can permit dictionary attacks for public recipients; opaque references can still correlate activity. Keep raw shielded Unified Addresses, seeds, spending/viewing keys and unneeded RPC payloads outside agent-facing data and logs. Retain commitment, nonce/domain and inclusion/proof references needed for audit/reconciliation. Local store files are sensitive metadata and require host access control; they are not encrypted or anonymized. Data snapshots bound nesting but not object/array width: a production gateway must enforce request-byte and collection-size limits before these APIs, as well as strict duplicate-key parsing.

Only classical native chain authentication is used. SHA-256 commitments are not signatures and do not supply post-quantum authorization. No PQ/hybrid signature, KEM, key lifecycle, downgrade resistance or recovery implementation is active. Future ML-DSA/SLH-DSA signature and ML-KEM key-establishment evaluation requires versioned algorithm/identity profiles, reviewed implementations and interoperability/resource/security tests before any PQ claim. ML-KEM is not a signature. See the comprehensive [Phase 4 security model and incident runbook](../SECURITY.md).

## Interpreting a testnet check or dashboard

A read-only RPC probe may show endpoint reachability, reported network identity or current block data. An unsigned serializer may show transaction shape. Neither authenticates an agent, consumes on-chain policy, proves canonical finality, submits a transaction or verifies payment. An RPC outage, unavailable Zcash lightwalletd, unsupported route or missing native verifier must be shown as a gap/error, never silently replaced with fabricated live evidence. Use only synthetic/local data for deterministic demo scenarios and label simulated authorization, submission, finality and reconciliation explicitly. Do not load private agent memory, wallet secrets or raw shielded recipients into a public page.
