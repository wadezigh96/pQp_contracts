# QPC NEAR execution-proof design (research-backed, not yet a light client)

## Status

This document specifies the next trust-minimization milestone for the QPC NEAR → Ethereum agent-payment route. It is not an implementation claim. The current Ethereum settlement contract still uses a validator quorum. The current NEAR escrow emits proof-friendly events, but Ethereum does not yet verify NEAR consensus or execution proofs.

## Why the payment event was expanded

A light client must be able to prove the exact payment fields that Ethereum will settle. The previous QPC event exposed only a payment ID, which was insufficient to bind the destination token, payer, recipient, and amount to the proven source execution outcome.

The NEAR escrow now emits these fields in its payment_escrowed event:
- payment_id
- token_id
- payer
- recipient
- amount (decimal string)
- created_at_ns (decimal string)
- expires_at_ns (decimal string)

All integer amounts/timestamps are serialized as decimal strings to avoid JSON number precision loss. Event fields must be parsed strictly; unknown schema versions and missing/duplicate fields fail closed.

## Protocol layers

### A. NEAR consensus light client

An Ethereum verifier must maintain a trusted finalized NEAR head and the producer set for the relevant epoch. Header updates must validate:
- monotonically increasing block height;
- allowed epoch transition from the current head;
- reconstructed current/next block hashes using NEAR's specified Borsh encodings and SHA-256;
- approval signatures against the correct epoch producer keys;
- strictly more than two-thirds of total producer stake;
- next_bps hash against next_bp_hash when crossing an epoch.

A relayer may submit headers, but must not be trusted to decide whether a header is valid. The contract/verifier must enforce the protocol rules. Epoch transitions and validator-set storage are consensus-critical.

### B. Execution outcome proof

After the head is trusted, verify a NEAR execution outcome proof against the trusted header:
1. Reconstruct the shard outcome root from the execution outcome and its Merkle path.
2. Reconstruct the block outcome root using the outcome-root Merkle path.
3. Match the result to the outcome_root in the proven block header.
4. Reconstruct the block hash and verify its inclusion in the trusted head's block Merkle root.
5. Require the execution outcome to be successful and the executor/receiver to be the configured QPC escrow account.
6. Parse exactly one QPC payment_escrowed event with standard qpc, version 1.0.0, and all required payment fields.
7. Bind the proven payment ID, NEP-141 token account, payer, recipient, and amount to the destination settlement statement. Apply an explicit token mapping and decimal conversion policy; never assume NEP-141 and ERC-20 units are identical.

A transaction inclusion proof alone is insufficient: the protocol needs the execution outcome proving the escrow contract actually emitted the successful event.

### C. Ethereum settlement

The final destination payout must be bound to source chain domain, trusted NEAR head hash and height, source receipt/transaction identifier, QPC escrow account, payment ID and proven event payload, destination chain ID and vault address, destination token, recipient, amount, and expiry.

Consume a unique source payment key and source execution reference before token interaction. Any payout failure must revert the entire EVM transaction so replay keys are not consumed.

## Cryptography and implementation strategy

NEAR block approvals currently use Ed25519 signatures and stake-weighted consensus. NEAR's [post-quantum roadmap](https://www.near.org/blog/near-quantum-safe-roadmap) distinguishes account transaction signing from validator consensus; ML-DSA-65 access keys therefore do not make this light-client verification path post-quantum. A direct Solidity implementation must correctly handle NEAR public-key/signature encodings, Borsh serialization, SHA-256 header hashing, epoch transitions, and stake threshold arithmetic. This is not interchangeable with the ECDSA validator-quorum signatures currently used by QPC.

Recommended route:
1. Build a reproducible off-chain reference verifier in Rust against official NEAR light-client data structures and fixtures.
2. Differential-test it against nearcore/RPC proof outputs, including epoch transitions, invalid signatures, malformed Borsh, fork/stale headers, and invalid Merkle paths.
3. Choose an on-chain verification strategy: a rigorously audited native Solidity verifier if gas and signature verification are practical, or a succinct proof system whose circuit verifies the exact same NEAR consensus and execution-proof rules.
4. Only after independent review, add a proof-verifier interface to Ethereum settlement and disable validator-quorum fallback for the proof-verified route.
5. Run local EVM tests, NEAR Sandbox tests, and testnet end-to-end tests with valueless test tokens.

A generic contract that calls an owner-selected verify(bytes) function is only a trust indirection. It is not a light client. A mock verifier in tests proves interface wiring only, never cryptographic correctness.

## Required adversarial tests

- Header height regression and same-height conflicting head.
- Invalid/duplicate validator approvals and exactly-two-thirds (must fail; requirement is greater than two-thirds).
- Wrong epoch producer set and invalid next_bps hash.
- Borsh field-order/endianness corruption.
- Incorrect current/next block hash.
- Invalid outcome proof, outcome-root proof, and block Merkle proof.
- Failed execution outcome, wrong escrow executor, wrong receipt ID.
- Missing, duplicated, malformed, wrong-version, or forged QPC event.
- Mismatch in payment ID, token, payer, recipient, amount, or expiry.
- Replay across destination vaults/chains and across source payment IDs/receipts.
- Chain reorg/finality boundary and stale trusted head.

## Primary protocol references

- NEAR Nomicon light-client specification: https://nomicon.io/ChainSpec/LightClient.html
- NEAR Rainbow Bridge specification: https://github.com/Near-One/rainbow-bridge/blob/master/SPEC.md
- NEAR official Rainbow Bridge design overview: https://www.near.org/blog/eth-near-rainbow-bridge

These references describe the consensus light-client header flow and the separate execution-outcome proof flow. QPC must implement both; validating a validator quorum is not equivalent.

## Security statement

Until a production-grade verifier is implemented, independently tested, audited, and connected to settlement, QPC remains a validator-attested experimental payout vault. It is not a trustless bridge and does not provide a cryptographic guarantee against malicious validators. No ML-DSA/SLH-DSA post-quantum verification is integrated.
