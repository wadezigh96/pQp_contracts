# QPC Zcash Agent Adapter

Fail-closed Zcash address, wallet-storage, and payment-policy boundary.

## Implemented

- Parses ZIP-316 Unified Addresses with librustzcash `zcash_address`.
- Checks Mainnet/Testnet using `zcash_protocol`.
- Extracts Orchard, Sapling, and transparent receiver capabilities.
- Requires an opaque recipient reference separate from the raw Unified Address.
- Enforces shielded-only intent policy, expiry, recipient binding, replay guard, and receipt-gated lifecycle.
- Uses official librustzcash light-wallet crates: `zcash_client_backend 0.24.0` and `zcash_client_sqlite 0.22.0`.
- Initializes/migrates a local Testnet SQLite wallet database without importing or generating keys.

## Strict current boundary

The wallet initializer is storage-only. It does not generate/import seed material, derive a Unified Address, connect to lightwalletd, sync shielded notes, build/sign transactions, broadcast, track confirmations, or verify receipts. No real NEAR ↔ Zcash payment has been completed.

Spending keys must remain in a dedicated local wallet boundary and never enter agent prompts, NEAR contract state, public events, route logs, or CI output. The SQLite wallet database must be stored in private app storage with restrictive permissions. Do not commit wallet databases, seeds, viewing keys, or addresses.

## Next integration stages

1. Add a testnet lightwalletd connection with pinned TLS roots and explicit endpoint allowlisting.
2. Add view-only synchronization and shielded balance/receipt observation.
3. Add an explicit human-authorized Orchard transaction proposal flow.
4. Add a separate signer boundary and testnet-only broadcast gate.
5. Verify transaction inclusion and confirmations independently before reporting settlement.

CI tests only local storage initialization and policy/parser behavior. It does not access a live network or send funds.


## Optional private explorer query boundary

The `privacy_query` module accepts only bounded opaque ciphertext payloads and exposes a read-only transport trait. It intentionally has no ordinary HTTP explorer fallback: a PIR outage must fail closed rather than disclose a transaction ID, block identifier, or address through a plaintext query.

ZecBlock's public API reference documents normal `https://api.zecblock.com/v1` reads. The ZecBlock Private announcement describes browser-side PIR but does not publish a stable public machine-to-machine request schema in that reference. Therefore this repository does not claim a live ZecBlock PIR integration and does not invent its cryptographic wire format or endpoint. A reviewed vendor protocol/client is required before implementing the transport.

PIR protects query contents from the service under its protocol assumptions; it does not hide client IP address, timing, request size, or the underlying public blockchain transaction. It is not a shielded-payment or settlement verifier.
