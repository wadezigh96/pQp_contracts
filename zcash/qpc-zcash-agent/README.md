# QPC Zcash Agent Adapter

This is a fail-closed adapter boundary and local state-machine prototype.

## Current scope

- Models Zcash Mainnet/Testnet explicitly.
- Requires an opaque recipient reference rather than exposing a Unified Address in QPC intents.
- Enforces shielded-only policy by requiring Orchard or Sapling receiver capability.
- Rejects expired intents, wrong-network recipients, recipient-reference substitution, and replayed source-chain nonces.
- Requires receipt verification before a payment can reach `Completed`.
- Keeps wallet parsing, signing, broadcasting, and consensus/receipt verification behind traits.

## Deliberate limitations

This crate does **not** yet parse real Unified Addresses, create or sign transactions, broadcast to Zcash testnet, inspect confirmations, or verify a shielded receipt. The traits are integration seams, not working wallet implementations. Do not send funds based on this prototype.

A production backend must use maintained Zcash libraries, keep spending keys inside a dedicated wallet boundary, validate network and receiver types, and provide independently reviewed confirmation/receipt verification. Shielded status must be derived from transaction data, not provider-supplied labels.
