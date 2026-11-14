# QPC Zcash Agent Adapter

Fail-closed Zcash address, wallet-storage, and payment-policy boundary.

## Implemented

- Parses ZIP-316 Unified Addresses with librustzcash `zcash_address`.
- Checks Mainnet/Testnet using `zcash_protocol`.
- Extracts Orchard, Sapling, and transparent receiver capabilities.
- Creates an opaque recipient handle internally; the raw Unified Address is held in a private, non-Debug capability and is not agent-visible.
- Enforces shielded-only intent policy, expiry, recipient binding, persistent replay reservation, and receipt-gated lifecycle.
- Rejects `AllowTransparentExplicitly` because a separate human-authorization and transparent-signing path does not yet exist.
- Persists replay keys using SHA-256-named files and atomic create-new semantics in a private 0700 directory; the store must survive process restarts.
- Uses official librustzcash light-wallet crates: `zcash_client_backend 0.24.0` and `zcash_client_sqlite 0.22.0`.
- Initializes/migrates a local Testnet SQLite wallet database without importing or generating keys.
- Adds a read-only lightwalletd Testnet probe over HTTPS with exact authority allowlisting, TLS roots, bounded RPC timeouts, and chain-name validation. It reads only server info and current tip; it cannot submit transactions.

## Strict current boundary

The wallet initializer remains storage-only. The new lightwalletd probe verifies HTTPS connectivity and Testnet identity and reads chain-tip metadata only; it does not yet scan shielded notes, calculate wallet balances, or verify payment receipts. No seed generation/import, transaction construction/signing, or broadcast is implemented. No real NEAR ↔ Zcash payment has been completed.

Spending keys must remain in a dedicated local wallet boundary and never enter agent prompts, NEAR contract state, public events, route logs, or CI output. The SQLite wallet database and replay-store directory must be stored in private app storage. Do not commit wallet databases, seeds, viewing keys, or addresses. The replay directory is integrity-critical: deleting or restoring an older copy can re-enable a previously used nonce; back it up atomically and do not roll it back.

## Next integration stages

1. Add view-only synchronization using a separately imported viewing capability and verified chain-state checkpoints.
2. Add shielded balance/receipt observation without exposing viewing keys to agents.
3. Add an explicit human-authorized Orchard transaction proposal flow.
4. Add a separate signer boundary and testnet-only broadcast gate.
5. Verify transaction inclusion and confirmations independently before reporting settlement.

CI tests only local storage initialization, filesystem protections, policy/parser behavior, and persistent replay reservation. It does not access a live network or send funds.


## Optional private explorer query boundary

The `privacy_query` module accepts only bounded opaque ciphertext payloads and exposes a read-only transport trait. It intentionally has no ordinary HTTP explorer fallback: a PIR outage must fail closed rather than disclose a transaction ID, block identifier, or address through a plaintext query.

ZecBlock's public API reference documents normal `https://api.zecblock.com/v1` reads. The ZecBlock Private announcement describes browser-side PIR but does not publish a stable public machine-to-machine request schema in that reference. Therefore this repository does not claim a live ZecBlock PIR integration and does not invent its cryptographic wire format or endpoint. A reviewed vendor protocol/client is required before implementing the transport.

PIR protects query contents from the service under its protocol assumptions; it does not hide client IP address, timing, request size, or the underlying public blockchain transaction. It is not a shielded-payment or settlement verifier.


## Upstream ZecBlock PIR research (source audit)

Audited the public upstream source at `Kenbak/zecblock`, commit `41ac8b2b2a9dff856486a9d229d748dcb8328068`, specifically `prototype/pir-browser`.

Findings that matter for QPC:

- This is explicitly a local feasibility prototype, not a public production PIR service/client API.
- Its README says it does not implement the production wallet-PIR HTTP envelope, private address history, or mempool retrieval.
- The browser client is Rust compiled to WASM and uses pinned upstream `zakura-pir-client` / `zakura-pir-protocol` sources; the bootstrap script verifies upstream source revisions and archive hashes.
- The basic fixture mode covers a bounded 2,048-row transaction-directory fixture. The separate full-confirmed-snapshot mode has a fixed profile and a 1–64 chunk query schedule.
- The host must create the complete fixed query plan before decoding any response; interleaving decode and later query creation is rejected by the upstream client. QPC must preserve this scheduling rule if it ever adopts the client.
- The source itself warns that successful PIR decoding does not authenticate publisher chain data, prove block inclusion, or prove complete coverage. QPC must separately verify chain inclusion/finality.
- The source says the host transport, publication freshness, generation retention, and no-fallback policy remain host-application responsibilities.

This is an upstream source-code review, not a live integration. QPC does not vendor or execute this prototype. Before reuse, review each upstream component's license and transitive dependency licenses, pin exact source hashes, reproduce its build and test results, and threat-model the publication/evaluator/transport path. Do not assume the root repository license automatically governs every downloaded dependency.

### QPC adoption gates

1. Reproduce upstream's pinned build and tests in an isolated audit job.
2. Establish an officially supported production publication/transport contract with ZecBlock; do not point the local prototype at an undocumented production endpoint.
3. Implement the fixed-plan transport with request/response size checks, publication identity pinning, timeout limits, replay protection, and no plaintext fallback.
4. Treat PIR output as an explorer observation only; independently verify transaction inclusion and confirmations using a trusted Zcash light client/node.
5. Keep PIR query metadata separate from wallet keys, shielded note sync, and transaction signing.


## Current security limits

- The persistent replay guard is a local single-device store, not a distributed consensus service. A multi-host deployment must use a shared transactional store with a unique constraint before accepting cross-host payment intents.
- A private opaque recipient handle reduces accidental disclosure in agent-facing structures; it is not an encryption system and does not protect process memory from a compromised host.
- Transparent payments remain disabled until a separately authenticated human-consent flow and a dedicated, reviewed transparent transaction path are implemented.
- The state machine is a policy helper only; it is not durable settlement state. Do not report completion without independently verified Zcash chain inclusion and confirmation.
