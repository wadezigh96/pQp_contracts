# QPC Agent Router — NEAR to Ethereum

This package is the first executable agent-operated cross-chain payment adapter for QPC.

## Current scope

- Uses NEAR Intents 1Click for a NEAR-origin to Ethereum-destination route.
- Reads token identifiers from the live token catalogue; it never invents asset IDs.
- Requests an exact-input quote and validates amount, deadline, deposit address, and minimum output.
- Requires a QPC policy-gate callback before and after quote commitment.
- Builds an unsigned NEP-141 ft_transfer wallet transaction to the quote-specific NEAR deposit address.
- Submits the source transaction hash to 1Click and supports status verification.
- Requires independent destination-chain receipt verification before reporting SETTLED.

## Important security boundary

This package does not hold keys and does not sign transactions. A caller must supply an explicit NEAR wallet signer. The wallet remains responsible for key custody and transaction approval.

The policyGate callback is an integration seam, not proof that the current QPC NEAR contract already enforces this payment flow on-chain. The current QPC policy contract does not yet expose a dedicated escrow/payment-router method or token-balance accounting. Do not connect this package to an unattended production wallet until that on-chain enforcement is implemented and audited.

NEAR Intents 1Click is an external settlement provider. QPC does not operate the bridge, guarantee solver liquidity, or make the provider trustless. The agent must verify the destination receipt independently. Provider SUCCESS alone is not sufficient.

This first adapter supports NEP-141 tokens on NEAR to Ethereum assets only. Native NEAR, BSC, Solana, reverse routes, cross-chain escrow, and automatic retries are not implemented here.

## Usage

~~~js
import {
  prepareNearToEthereumPayment,
  submitNearDeposit,
  getPaymentStatus,
  verifyPaymentStatus,
} from "@qpc/agent-router";

const prepared = await prepareNearToEthereumPayment({
  originAssetId: process.env.QPC_NEAR_ASSET_ID,
  destinationAssetId: process.env.QPC_ETH_ASSET_ID,
  amount: "1000000",
  recipient: "0x0000000000000000000000000000000000000001",
  refundTo: "agent.near",
  slippageTolerance: 100,
  policyGate: async (context) => qpcPolicyAdapter.authorize(context),
});

// Display prepared.sourceTransaction to the wallet approval UI.
// Only after explicit wallet approval:
const submitted = await submitNearDeposit(prepared, {
  nearSenderAccount: "agent.near",
  signAndSendTransaction: (tx) => nearWallet.signAndSendTransaction(tx),
});
~~~

## Tests

Run npm test. Tests use mocked API responses and wallet callbacks. They do not contact a live provider, sign transactions, or move funds.

## References

- https://docs.agentsonintents.com/integration/distribution-channels/1click-api/about-1click-api
- https://docs.agentsonintents.com/integration/distribution-channels/1click-api/quickstart/making-a-request
- https://github.com/near/agent-skills/tree/main/skills/near-intents
