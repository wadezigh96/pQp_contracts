# QPC NEAR — Chain-Native Contracts

QPC NEAR is an independent NEAR-native implementation. It does not share bytecode or state with Ethereum, BNB Smart Chain, or Solana.

## Contracts

- `qpc-near`: policy and constrained asynchronous execution core.
- `qpc-near-escrow`: separate NEP-141 same-chain agent-to-agent escrow prototype.

## Same-chain escrow flow

1. Owner allowlists NEP-141 token contracts and agent accounts.
2. Agent A calls the token's `ft_transfer_call` with the QPC escrow contract as receiver and a JSON payment intent.
3. The escrow verifies the actual token contract (predecessor), authenticated sender, token cap, unique payment ID, recipient, and expiry.
4. Agent B claims before expiry; or Agent A refunds after expiry.
5. The contract initiates an NEP-141 `ft_transfer`; a private callback records success or restores Pending after a failed transfer.

The contract accepts only allowlisted tokens and registered payer agents. It holds tokens in custody while a payment is Pending. This is a same-chain escrow, not a bridge, cross-chain messaging protocol, or proof of cross-chain settlement.

## Storage and operations limits

The prototype caps lifetime escrow creations at 10,000 because payment records are retained permanently. The contract account must be funded with enough NEAR for storage and gas before use. This is not a storage-deposit standard and has not been load-tested. Do not enable unrestricted agent registration or token lists.

A payout enters `ReleasePending` or `RefundPending` before the external FT call. If the callback itself cannot execute after a successful FT transfer, the record may require manual reconciliation; never retry a pending payout blindly. This limitation must be resolved with runtime integration tests and recovery design before production.

## Build and test

```sh
cd near
cargo test --workspace
cargo check --workspace
cargo build --release --target wasm32-unknown-unknown -p qpc-near
cargo build --release --target wasm32-unknown-unknown -p qpc-near-escrow
```

The `qpc-near` unit/integration suite includes `tests/cyber_security.rs` for fail-closed agent controls, policy-version safety, payload bounds, nonce replay protection, callback integrity, and preserved accounting. `tests/cross_chain_privacy.rs` verifies that the cross-chain fixture carries only an opaque recipient reference, binds the exact amount and privacy payload to the policy hash, rejects router redirection, and exposes no transparent action when only the shielded rule is configured.

These tests validate the controls currently implemented by the contract. They do not claim that the contract already implements Chain Signatures, a private L1 transaction, a TEE, a solver, or a receipt verifier. Those are explicit integration boundaries and must be tested separately when implemented.

## Safety boundary

This is an experimental prototype, not audited or production-ready. No mainnet deployment or live payment is authorized. NEAR supports ML-DSA-65 access keys for transaction signing, but this contract does not implement an in-contract ML-DSA/SLH-DSA verifier. Runtime acceptance of an ML-DSA-signed transaction is not QPC contract-level signature verification and does not make NEAR consensus or cross-chain signing post-quantum. See [NEAR's access-key documentation](https://docs.near.org/protocol/accounts-contracts/access-keys) and the [QPC experiment plan](POST_QUANTUM_EXPERIMENTS.md).
