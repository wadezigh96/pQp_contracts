# QPC NEAR Agent Finance Contract

Rust/NEAR contract MVP for deterministic authorization of QPC agent payment intents.

## Scope

This crate is a policy enforcement and audit-recording boundary. It does not call a Zcash router, sign wallet transactions, broadcast payments, or verify shielded settlement.

## Implemented

- Owner-only agent policy configuration and revocation.
- Bound router account, owner-approved opaque-recipient allowlist, policy ID/version, expiry, per-payment limit, cumulative cap, and monotonic per-agent nonce.
- Revocation keeps the nonce/version record; reactivation increments the policy version without resetting the replay counter.
- Fixed action (pay_zcash_shielded), destination (zcash:testnet), and privacy mode (SHIELDED_ONLY).
- Canonical payment-argument hash verification using the same domain and JSON field order as agent-router/src/index.mjs.
- Payment-ID replay rejection and spending accounting.
- NEP-297-style authorization event that omits the opaque recipient reference.
- Unit tests for valid dispatch, owner authorization, unconfigured callers, router substitution, argument tampering, nonce replay, expiry, and amount limits.

## Deliberate limitations

- dispatch_action records an authorized intent; it is not a cross-contract call and does not mean funds moved.
- There is no NEAR escrow, receipt verifier, refund mechanism, or cross-chain atomicity.
- The current fixed destination is Zcash Testnet. No Mainnet settlement is enabled.
- This MVP has not been deployed to NEAR Testnet, independently audited, or reviewed for production gas/storage economics.
- The contract does not provide private NEAR state/events or post-quantum authentication.

## Build and test

From the repository root:

- cargo fmt --manifest-path near/qpc-near-agent-finance/Cargo.toml -- --check
- cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml
- rustup target add wasm32-unknown-unknown
- cargo build --manifest-path near/qpc-near-agent-finance/Cargo.toml --target wasm32-unknown-unknown --release

CI runs formatting, unit tests, and the WASM build. No wallet key, signer, deployer, or RPC credential is needed for these checks.
