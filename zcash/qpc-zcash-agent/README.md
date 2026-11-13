# QPC Zcash Agent Adapter

A fail-closed address and payment-policy boundary for QPC agentic payments.

## Implemented

- Parses ZIP-316 Unified Addresses with librustzcash zcash_address.
- Checks Mainnet/Testnet through zcash_protocol.
- Extracts Orchard, Sapling, and transparent receiver capabilities.
- Requires an opaque recipient reference separate from the raw Unified Address.
- Enforces shielded-only intent policy, expiry, recipient binding, replay guard, and receipt-gated lifecycle.

## Not implemented

This crate is not yet a Zcash wallet. It does not derive keys or addresses, sync shielded notes, construct/sign transactions, broadcast to testnet, track confirmations, or verify shielded receipts. No real NEAR ↔ Zcash payment has been completed.

A future wallet backend must keep spending keys outside agent processes, use a supported Zcash light-client backend, and expose only narrowly scoped operations. Receipt verification must not rely solely on a hosted provider's success callback.

## CI scope

The workflow currently tests only this crate because the agent-router/ and near/qpc-near/ directories referenced by the previous workflow are absent from this branch. Cross-chain integration jobs must be added only when those components exist and have runnable tests.
