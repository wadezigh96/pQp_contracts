# Local-only verification, operations and demo

No deployment, wallet signing, broadcast, transfers, merging or hackathon submission is authorized for this work. Nothing in this guide is evidence of a deployed product.

## Reproducible unsigned demo

Run `node --test agent-router/test/*.test.mjs`. The public API tests construct canonical envelopes, compare a literal cross-language vector, authenticate only through explicitly supplied test boundaries, prepare unsigned native requests and reject adversarial substitutions. The local lifecycle tests use temporary directories, concurrent store instances and restart reads. Mock evidence returns `MOCK_VALIDATED_NOT_SETTLED`; production native settlement deliberately throws. No network or wallet is used by this demo.

Run `cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml` for native predecessor authorization and policy/replay enforcement in a mocked NEAR runtime. A mocked runtime is not a deployed NEAR node. Run `cargo build --manifest-path near/qpc-near-agent-finance/Cargo.toml --target wasm32-unknown-unknown --release` to produce a local WASM artifact only.

Run `forge fmt --check`, `forge build --sizes` and `forge test --summary` for Solidity unit, security and stateful/invariant tests. These use local Foundry EVM execution, not a network broadcast.

## Deployment boundary (not performed)

Before any future release: obtain separate deployment authority; review incompatible NEAR state-layout/API changes and migration; pin native network/profile/contract identities; use audited native signer and proof providers isolated from agents; provision production atomic storage with backup/restore testing; measure gas/storage and resource limits; verify deployed WASM/bytecode and constructor/owner; validate canonical vectors across released clients; independently audit crypto and economic behavior. Local build success does not establish deployability, production performance or consensus verification.

## Failure handling

Do not automatically resubmit after an uncertain broadcast. Persist the intent and expected request digest before external work. Treat timeout, partial execution, conflicting inclusion or lost finality as reconciliation. Cancellation and expiry before submission never free consumed nonce/intent keys. Reconciliation records are evidence references, not refunds; no refund/compensation protocol is implemented. Never infer that a provider callback, transaction hash, local record or success-shaped object proves external settlement.

## Privacy and quantum posture

Public calldata, contract state, predecessor/account links, amounts, timestamps, policy profiles, transaction IDs and recipient commitments remain observable. A deterministic recipient commitment can permit dictionary attacks for public recipients; opaque references can still correlate activity. Keep raw shielded Unified Addresses, seeds, spending/viewing keys and unneeded RPC payloads outside agent-facing data and logs. Retain commitment, nonce/domain and inclusion/proof references needed for audit/reconciliation. Local store files are sensitive metadata and require host access control; they are not encrypted or anonymized. Data snapshots bound nesting but not object/array width: a production gateway must enforce request-byte and collection-size limits before these APIs, as well as strict duplicate-key parsing.

Only classical native chain authentication is used. SHA-256 commitments are not signatures and do not supply post-quantum authorization. No PQ/hybrid signature, KEM, key lifecycle, downgrade resistance or recovery implementation is active. Future migration requires versioned algorithm/identity profiles, reviewed standardized implementations and interoperability/resource/security tests before any PQ claim.
