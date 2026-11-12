# QPC Agent-to-Agent Cross-Chain Payment Network

**Status: architecture specification only. This document does not claim that cross-chain routing, escrow, bridge integration, or agent runtime is implemented or deployed.**

## Product definition

QPC is intended to let **each agent that adopts QPC on any supported chain initiate and receive payments on other supported chains**.

Example: Agent A operates from NEAR and holds funds on NEAR. Agent B operates on Ethereum and registers an Ethereum receiving address. Agent A instructs its QPC-enabled agent runtime to pay Agent B. The runtime obtains a route, authorizes the exact payment under QPC policy, settles on NEAR, and causes the agreed value to arrive at Agent B's Ethereum address through a separately verified interoperability/settlement network.

This is not merely cross-chain chat or a directory of agents. It is an **agent-operated cross-chain payment protocol**: the agents choose and execute payment intents within explicit policy limits; chain-native contracts enforce the on-chain parts; an established interoperability/solver/bridge mechanism transports or rebalances value.

QPC is not itself a bridge, exchange, custodian, or universal token. Do not describe it as trustless or atomic until the selected route actually provides those properties.

## Core model: one agent, multiple chain payment endpoints

An agent has one logical QPC agent identity and may register multiple chain-specific endpoints:

- NEAR account ID and QPC NEAR policy/escrow references.
- Ethereum address and Ethereum QPC/escrow references.
- BNB Smart Chain address and BSC QPC/escrow references.
- Solana public key and Solana QPC program/escrow references.

The identity-to-endpoint binding must be authenticated by the agent and independently verifiable. A registry entry is not proof of ownership by itself. Endpoint rotation, revocation, recovery, and compromised-key handling must be defined.

Each endpoint is chain-specific. Balances are not automatically unified, and a NEAR account cannot directly sign an Ethereum transaction. An agent runtime needs authorized signing capability for the source chain and a verified settlement route to the destination chain. It must not export or reuse one chain's private key as another chain's key.

## Components

1. **Agent identity and endpoint registry:** signed agent manifest maps one logical agent identity to verified chain endpoints, supported assets, service capabilities, and key references.
2. **Agent runtime / payment router:** runs with the agent's authorized wallet capabilities; discovers recipient endpoints, requests quotes, compares routes, constructs a typed payment intent, and monitors settlement. It cannot exceed QPC policy.
3. **Chain-native QPC policy adapter:** separate implementation on NEAR, Ethereum, BSC, and Solana. Checks agent, recipient, asset, amount, limits, expiry, nonce, and permitted route/protocol.
4. **Chain-local escrow or payment adapter:** locks/transfers source funds and handles destination release/claim where applicable. Kept separate from policy logic.
5. **Interoperability / liquidity network:** an established bridge, intent solver, messaging protocol, or liquidity provider transports value or coordinates source/destination settlement. Its trust model must be explicit and reviewed. QPC must not invent its own bridge.
6. **Settlement monitor and evidence store:** independently verifies source and destination chain events, finality, and job IDs; handles retries, reorgs, timeouts, and disputes.

## Example: Agent A on NEAR pays Agent B on Ethereum

1. **Recipient registration:** Agent B publishes a signed receiving manifest binding its logical agent ID to an Ethereum address, accepted token(s), minimum/maximum amount, and expiry. Agent A verifies the manifest and key trust.
2. **Payment intent:** Agent A requests payment of a specified amount and destination asset to Agent B. Intent binds protocol version, unique payment ID, source NEAR account, destination chain ID (Ethereum), destination QPC/escrow identifier, recipient address, source and destination assets, amount/minimum received, quote expiry, nonce, and refund destination.
3. **Route quote:** the router asks supported interoperability solvers/providers for executable quotes. Quotes disclose source amount, destination amount, fees, slippage, route/provider, expected settlement time, expiry, and failure/refund conditions. Agent A rejects routes outside policy.
4. **QPC authorization:** the NEAR QPC policy adapter checks the source agent, destination agent/address, allowed destination chain, asset, maximum source spend, minimum destination receive, daily/job cap, nonce, and deadline. The payer's configured approval policy is applied. AI output alone never approves funds.
5. **Source settlement:** Agent A signs and submits the NEAR-side transaction. Source funds are transferred to the selected audited escrow/intent contract or otherwise committed using the provider's documented mechanism. The runtime waits for the required NEAR finality evidence.
6. **Cross-chain execution:** the selected provider verifies/observes source settlement and executes its documented destination release, mint, or liquidity payment on Ethereum. QPC does not accept an unverified HTTP callback as proof.
7. **Destination verification:** Agent B's runtime or an independent monitor verifies the Ethereum transaction, correct chain, token contract, recipient, amount received, and finality. The payment is marked complete only after those checks.
8. **Failure handling:** if the route expires or fails, the system follows the provider's documented refund/recovery path. It must never mark both source refund and destination payment as successful for the same payment ID. Ambiguous states become `PendingReconciliation`, not success.
9. **Receipt:** both agents retain the same payment ID, source transaction, destination transaction, provider/route ID, amounts, fees, and final status.

If the source and destination asset differ, the quote must specify the conversion and minimum amount received. A bridge transfer and a cross-chain swap are different operations and must be identified accurately.

## Same-chain and cross-chain modes

- **Same-chain:** direct transfer or chain-local escrow; no bridge is involved.
- **Cross-chain, same asset:** source-to-destination transfer using a supported bridge or interoperability route.
- **Cross-chain, different asset:** intent/solver or exchange route with explicit conversion quote, fees, slippage, and minimum received.
- **No supported route:** reject safely. Never pretend that an agent can pay on a chain where it has no supported endpoint or settlement route.

## Network separation

| Network | QPC adapter | Native payment layer | Agent endpoint |
|---|---|---|---|
| NEAR | Independent Rust contract | NEAR native asset / NEP-141 FT | NEAR account |
| Ethereum | Solidity contract | ETH / allowlisted ERC-20 | EVM address |
| BNB Smart Chain | Separate BSC deployment/configuration | BNB / allowlisted BEP-20-compatible token | BSC address |
| Solana | Independent Rust/Anchor program | SOL / allowlisted SPL token | Solana public key |

Each network has its own chain/program ID, contract address, token registry, authority, nonce domain, event format, finality rules, and release process. A shared agent identity does not merge chain state. Ethereum and BSC may share reviewed EVM source components, but deployments and configuration remain separate. Solana and NEAR are not Solidity deployments.

## Security invariants

- Bind every payment to source chain, destination chain, source/destination QPC or escrow identifiers, both agent identities, exact recipient, source/destination token identifiers, decimals, amounts, fee ceiling, minimum received, expiry, and unique payment ID.
- Use typed canonical payloads and domain-separated signatures. Do not rely only on hashing arbitrary JSON.
- Maintain replay protection independently on every chain and across route providers.
- Require source and destination finality evidence appropriate to each network; handle reorgs and duplicate messages.
- Enforce per-agent, per-job, per-token, per-destination-chain, and per-provider limits.
- Restrict provider/bridge allowlists and support revocation and emergency pause.
- Treat provider/solver/relayer as an explicit trust assumption; never trust a plain HTTP success response.
- Model provider outage, liquidity failure, partial fill, fee changes, token freezes, malicious tokens, chain halt, reorg, and refund failure.
- Use safe ERC-20 handling and reentrancy protection; Solana signer/PDA/token-program/mint checks; NEAR attached-deposit, storage, private callback, and async recovery controls.
- Keep agent private keys out of contracts, manifests, repository files, CI, and prompts.
- AI model output is never transaction authorization. Payment execution must pass policy and configured approval controls.

## Agent autonomy and user control

An agent may autonomously discover routes, request quotes, compare providers, and prepare payment intents. Actual spending is bounded by owner-configured policy: per-payment cap, daily cap, approved counterparties, approved chains, approved assets, fee/slippage ceilings, and optional human confirmation thresholds. A high-risk or out-of-policy payment must stop for approval or reject. "Agent-operated" does not mean unrestricted wallet access.

## Post-quantum boundary

NEAR ML-DSA-65 transaction-key support, where enabled by the network protocol, authenticates NEAR transactions at protocol level. It does not automatically authenticate agent manifests or messages, nor does it make Ethereum, BSC, or Solana transactions post-quantum. SLH-DSA remains an off-chain experiment until a supported, reviewed verification path exists. Do not advertise cross-chain PQ authentication as active.

## Implementation sequence

### Phase 0 — specification and threat model
- Freeze endpoint manifest, typed payment intent, quote, receipt, and state machine.
- Define chain IDs, asset identifiers, decimals, finality requirements, and approval policy.
- Compare established interoperability providers; select none until trust and failure models are reviewed.
- No funds and no deployment.

### Phase 1 — local agent simulation
- Simulate Agent A on NEAR and Agent B on Ethereum with mock balances and deterministic provider adapter.
- Exercise discovery, quote, policy authorization, payment state transitions, duplicate messages, expiry, rejection, and reconciliation.
- Label all simulated transfers clearly; they are not cross-chain settlement.

### Phase 2 — NEAR same-chain escrow prototype
- Implement two-agent payment and escrow on NEAR Sandbox with mock FT.
- Test fund, accept, release, refund, timeout, replay, failed promise, and storage accounting.
- No external bridge.

### Phase 3 — first real cross-chain testnet route
- Select one established provider only after a threat model and documented integration review.
- Connect NEAR testnet source to Ethereum Sepolia destination only if the provider supports both test environments and the exact assets.
- Verify both transaction receipts independently.
- Never use mainnet funds for the first proof.

### Phase 4 — BSC and Solana adapters
- Add independent adapters and testnets with chain-specific security suites.
- Do not infer compatibility from EVM source sharing or Solana/NEAR Rust language similarity.

### Phase 5 — production hardening
- Independent audit of QPC adapters, escrow, provider integration, agent runtime, and key custody.
- Operational monitoring, incident response, route revocation, recovery drills, and public disclosure.

## Demo acceptance criteria

The first truthful cross-chain demo must show:
- Agent A running on NEAR and Agent B registered on Ethereum Sepolia;
- signed recipient endpoint ownership proof;
- a real provider quote with fees and minimum receive;
- QPC source-side policy authorization and configured approval;
- actual source transaction and actual destination transaction;
- independent verification of destination recipient/token/amount/finality;
- failure/replay/expired-quote behavior;
- complete evidence linked by one payment ID.

A local mock, architecture diagram, or simulated relayer is not a cross-chain payment demo. If no provider supports the selected testnet pair, report that limitation and keep the demo simulated.

## Current implementation status

The repository contains a NEAR policy/dispatch prototype and separate EVM work. This specification does not implement the agent runtime, endpoint registry, payment router, escrow, bridge/provider adapter, Solana program, or active PQ signature verifier. Cross-chain payment is a proposed capability, not an active feature. No deployment, wallet transaction, or real payment is authorized by this document.
