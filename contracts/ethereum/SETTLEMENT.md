# QPC Ethereum Settlement Vault (experimental)

This is the Ethereum-side destination payout adapter for the first NEAR → Ethereum agent-payment prototype.

## What it does

- Holds prefunded destination ERC-20 liquidity.
- Accepts settlement instructions only from one configured relayer.
- Requires an allowlisted token, nonzero recipient and amount, and a live deadline.
- Consumes both a source-payment key and a source-transaction reference to block duplicate settlement.
- Uses checks-effects-interactions and a reentrancy lock around token payout.
- Supports owner pause, token allowlisting, and relayer rotation.

## Critical trust boundary — NOT a trustless bridge

The relayer is trusted. The contract does not verify NEAR light-client proofs, NEAR finality, NEAR escrow state, 1Click solver execution, or Ethereum destination receipts. The relayer must independently verify the canonical source transaction and payment state before calling `settle`. Compromise or dishonesty of the relayer can drain the vault's allowlisted-token liquidity.

This is a destination payout vault with replay protection, not a bridge protocol and not a proof-verifying cross-chain contract. Do not fund it with valuable assets or deploy it to mainnet.

## NEAR source references

- `sourceDomain`: stable identifier such as `keccak256("near:mainnet")`.
- `sourcePaymentId`: unique ID emitted by the QPC NEAR escrow.
- `sourceTransactionRef`: `keccak256(UTF8(NEAR transaction hash string))`; the NEAR transaction hash is not a bytes32 value and must not be cast or truncated.

The relayer must check that the source escrow record is terminal and successfully released/settled according to the selected flow, and that the payment ID, payer, recipient intent, token mapping, and amount correspond to the authorized quote. A source escrow's `Released` state alone does not prove the external 1Click destination transfer.

## Test

```sh
forge test --match-contract EthereumQPCSettlementTest -vvv
```

The tests use a mock ERC-20 and Foundry's Sepolia chain ID locally. They do not connect to Sepolia, sign transactions, or move real funds.

## Not implemented

- NEAR light-client or succinct proof verification.
- Trust-minimized bridge messaging.
- Automated relayer service, threshold attestations, or fraud proofs.
- Token decimal/price normalization and route quote enforcement on-chain.
- BSC or Solana destination adapters.
- Mainnet deployment or production audit.

QPC does not claim post-quantum security. ML-DSA/SLH-DSA verification is not integrated.
