# QPC Zcash Adapter

**Status: privacy architecture and threat model only — no wallet or transaction implementation yet.**

This directory defines a separate Zcash wallet/transaction adapter for QPC agent payments. Zcash is a UTXO-based L1 with transparent and shielded pools; it is not an EVM smart-contract target. QPC must not describe this adapter as a deployed contract on Zcash.

## Privacy-first defaults

- Accept Unified Addresses (UA) and prefer shielded receivers, with Orchard preferred where the selected wallet stack supports it.
- Reject transparent-only destinations by default. No silent fallback to a transparent address.
- Never store or transmit seed phrases, spending keys, or viewing keys in QPC events, route messages, logs, or receipts.
- Do not publish raw Zcash recipient addresses in source-chain events.
- Keep memos empty by default; never place agent identity or personal data in them.
- Disclose what bridges, solvers, relayers, wallet providers, and network observers can correlate.
- Treat wallet-provider and lightwalletd trust assumptions explicitly; do not call them trustless verification.

## Documents

- [QPC Zcash Privacy Architecture and Threat Model](QPC_ZCASH_PRIVACY_ARCHITECTURE.md)

## Implementation gate

No production funds, real key material, or production claims until a maintained Rust wallet stack is selected and reviewed, UA and shielded transaction paths are implemented, confirmation/reorg handling is tested, privacy leakage tests pass, and an independent security review is complete.
