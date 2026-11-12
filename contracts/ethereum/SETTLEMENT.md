# QPC Ethereum Settlement Vault — validator quorum prototype

This is an experimental Ethereum destination payout vault for the NEAR → Ethereum agent-payment route.

## Quorum authorization

- A configured validator set signs a domain-bound settlement attestation.
- The vault requires at least `validatorThreshold` distinct active validator signatures.
- Signatures are EIP-191 personal-sign signatures over an attestation that binds source domain, source payment ID, source transaction reference, destination token, recipient, amount, deadline, destination chain ID, and vault address.
- Signers must be strictly ascending by address. Duplicate, inactive, malformed, high-s, and invalid-v signatures are rejected.
- The owner can add/remove validators and change the threshold. A validator cannot be removed if doing so would leave fewer active validators than the threshold.
- The contract has no privileged single relayer settlement path.

## Critical trust boundary — NOT a trustless bridge

The validator quorum still makes an off-chain trust assumption. Validators must independently verify NEAR finality, the QPC NEAR escrow record, payment intent, token mapping, amount, and recipient before signing. This contract does not verify NEAR light-client proofs, NEAR state proofs, 1Click solver execution, or Ethereum destination receipts.

A compromised threshold of validators, or a malicious owner changing the validator set, can authorize fraudulent payouts and drain prefunded liquidity. This is a threshold-attestation prototype, not a proof-verifying bridge and not a trustless bridge. Do not fund with valuable assets or deploy to mainnet.

## Source reference format

- `sourceDomain`: stable identifier such as `keccak256("near:mainnet")`.
- `sourcePaymentId`: unique payment ID emitted by QPC NEAR escrow.
- `sourceTransactionRef`: `keccak256(UTF8(NEAR transaction hash string))`. Do not cast or truncate NEAR transaction hashes into bytes32.

Validators must attest to exactly the same canonical payload. Token/decimal mapping and route quote validation remain policy responsibilities and are not inferred by this contract.

## Test

```sh
forge test --match-contract EthereumQPCSettlementTest -vvv
```

Tests use a mock ERC-20 and Foundry's Sepolia chain ID locally. They do not connect to Sepolia, sign live transactions, or move real funds.

## Not implemented

- NEAR light-client or succinct proof verification.
- Trust-minimized bridge messaging or fraud proofs.
- Automated independent validator clients and key management.
- BSC or Solana destination adapters.
- Mainnet deployment or production audit.

QPC does not claim post-quantum security. ML-DSA/SLH-DSA verification is not integrated.
