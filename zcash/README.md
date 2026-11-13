# QPC Zcash Adapter

**Status: Rust wallet-adapter prototype; not a production payment integration.**

This directory implements an early Zcash-specific wallet/transaction boundary for QPC agent payments. Zcash is a UTXO-based Layer 1 with transparent and shielded pools; it is not an EVM environment. QPC does not deploy a Solidity smart contract on Zcash. QPC authorization for Zcash actions is enforced by the agent runtime and durable adapter state.

## Present in this prototype

- Canonical Unified Address parsing and network validation.
- Shielded receiver selection policy (Orchard preferred; Sapling only when allowed).
- Transparent-only destination rejection; no silent transparent fallback.
- A private recipient wrapper with redacted debug output and empty memo default.
- SQLite wallet database initialization through zcash_client_sqlite.
- A separate SQLite compact-block cache and Testnet light-client sync entry point through zcash_client_backend.
- Durable SQLite payment-ID / nonce replay protection and a payment lifecycle model.

## Not implemented yet

- Seed, spending-key, or viewing-key import/custody.
- Shielded transaction construction, signing, or broadcast.
- End-to-end shielded receipt proof tied to a live recipient wallet.
- Live Testnet sync against a configured lightwalletd endpoint in CI.
- Bridge/solver integration, cross-chain settlement, refunds, or production routing.
- Independent Zcash consensus verification. lightwalletd remains a trust assumption.

## Privacy-first defaults

- Accept Unified Addresses and require a supported shielded receiver.
- Prefer Orchard; allow Sapling only when the configured policy explicitly supports it.
- Reject transparent-only destinations. Never silently fall back to a transparent address.
- Never store or transmit seed phrases, spending keys, or viewing keys in QPC events, route messages, logs, or receipts.
- Do not publish raw Zcash recipient addresses on source-chain events.
- Keep memos empty by default; never put agent identity or personal data in them.
- Disclose metadata that bridges, solvers, relayers, wallet providers, and network observers can correlate.

## Tests

From this directory, run:

    cargo fmt --all -- --check
    cargo test --locked
    cargo check --locked

These are local adapter tests, not proof of a live Testnet sync or real cross-chain settlement. The Testnet sync method requires an HTTPS lightwalletd endpoint and still trusts that service for data it does not independently verify.

## Documents

- [Agent-to-agent cross-chain payment specification](../AGENT_TO_AGENT_PAYMENTS.md)
- [QPC Zcash privacy architecture and threat model](QPC_ZCASH_PRIVACY_ARCHITECTURE.md)

No production funds, real key material, or production claims until shielded transaction construction, receipt verification, confirmation/reorg handling, privacy leakage tests, provider integration, and independent security review are complete.
