# QPC NEAR execution proof verifier (reference prototype)

This crate is an off-chain Rust reference verifier for NEAR execution outcome proofs used by the QPC NEAR → Ethereum research route.

## What it verifies

1. Parses a NEAR RPC `EXPERIMENTAL_light_client_proof` execution-proof response.
2. Checks the execution outcome's block hash, shard outcome Merkle path, outcome-root path, and block Merkle path using the upstream NEAR light-client protocol implementation.
3. Requires the execution outcome executor to equal the configured QPC escrow account.
4. Requires successful execution (`SuccessValue`).
5. Parses exactly one QPC `payment_escrowed` event with schema `qpc/1.0.0`.
6. Strictly parses payment ID, token account, payer, recipient, amount, creation time and expiry.

## What it does NOT verify

- It does not authenticate the supplied trusted block Merkle root.
- It does not sync NEAR headers, verify Ed25519 approvals, validate stake-weighted finality, or track epoch transitions.
- It does not establish token equivalence or decimal conversion to ERC-20.
- It is not an Ethereum verifier and is not a bridge.

The trusted root MUST come from a separately authenticated NEAR consensus light client. Passing an arbitrary RPC-provided root defeats the trust model. The official NEAR protocol crate is pinned to an immutable Git commit to make the reference implementation reproducible.

## Run

From the repository root:

```sh
cd near
cargo test -p qpc-near-proof
cargo run -p qpc-near-proof -- verify <trusted-head-block-merkle-root> <qpc-escrow-account> <proof.json>
```

The committed fixture is an upstream NEAR light-client inclusion-proof fixture. It validates the inclusion primitive only; it is not a QPC payment and must not be described as an end-to-end QPC payment verification.

## Next security milestone

Implement and test the authenticated NEAR header/epoch light client. Then produce a real QPC escrow transaction and request its execution outcome proof against a trusted finalized head. Only after independent differential tests should this logic be ported to an on-chain Ethereum proof verifier.
