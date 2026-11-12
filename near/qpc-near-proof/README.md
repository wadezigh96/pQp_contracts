# QPC NEAR execution proof verifier (reference prototype)

This off-chain Rust crate verifies NEAR execution outcome inclusion proofs for the QPC NEAR → Ethereum research route.

## What it verifies

- Reconstructs the NEAR block hash from the proven lite header.
- Reconstructs the shard outcome hash and the block outcome root using NEAR Merkle paths.
- Checks inclusion of the proven block hash under a caller-supplied trusted block Merkle root.
- Requires the execution outcome executor to equal the configured QPC escrow account.
- Requires successful execution (`SuccessValue`).
- Parses exactly one QPC `payment_escrowed` event with schema `qpc/1.0.0`.
- Strictly parses payment ID, token account, payer, recipient, amount, creation time and expiry.

The hash and Merkle procedure follows the NEAR light-client reference implementation in `near/near-light-client`, especially `crates/protocol/src/lib.rs` and `crates/protocol/src/merkle_util.rs`. NEAR primitive and RPC types are used directly from their 0.20 crates.

## Critical trust boundary

This crate does NOT authenticate the supplied root as a finalized NEAR consensus head. It does not sync headers, verify Ed25519 block approvals, validate stake-weighted finality, or track epoch transitions. Passing an arbitrary RPC-provided root defeats the trust model.

It also does not prove token equivalence or decimals conversion to ERC-20. It is not an Ethereum verifier and is not a bridge.

## Run

From repository root:

```sh
cd near
cargo test -p qpc-near-proof
cargo run -p qpc-near-proof -- verify <trusted-head-block-merkle-root> <qpc-escrow-account> <proof.json>
```

The committed fixture is an upstream NEAR light-client inclusion-proof fixture. It validates the inclusion primitive only; it is not a QPC payment and must not be described as end-to-end QPC payment verification.

## Next security milestone

Implement and test authenticated NEAR header/epoch synchronization, then produce a real QPC escrow transaction and request its execution outcome proof against a trusted finalized head. Only after independent differential tests should this logic be ported to an on-chain Ethereum verifier.


## Consensus light-client reference module

The `consensus` module wraps the pinned upstream `near-light-client-protocol`
implementation at commit `522f94003fbbedfdc1e7a7de6284f5a1d25d8134`.

It maintains a trusted header checkpoint and its epoch producer set, then
validates each subsequent `LightClientBlockView` through upstream
`Protocol::sync`. That protocol path checks strictly increasing height,
current-or-next epoch progression, approval signatures, stake-weighted
approval strictly greater than two-thirds, and the next validator-set hash
when the epoch changes.

QPC adds a defensive exact-length check for `approvals_after_next` before
calling upstream code. This matters because the upstream implementation's
stake fold pairs approvals and validators with `zip`; a truncated vector
must not silently shrink the denominator.

The checkpoint is a trust anchor, not something this module authenticates.
The caller must provision it through an independently trusted bootstrap
process. The module is an off-chain Rust reference component; it is not
deployed on Ethereum, BSC, Solana, or NEAR and does not make QPC a trustless
bridge. Tests that do not contain authentic signed NEAR block fixtures are
guard tests, not end-to-end consensus proof tests.
