# QPC Agentic Cross-Chain Payments

**Normative authorization specification:** [QPC Cross-L1 Authorization Envelope](QPC_CROSS_L1_AUTHORIZATION.md) defines the target fields and adapter verification requirements. The current implementation is narrower, as detailed below.

**Status: architecture and local test coverage; not a live bridge or production settlement network.**

## Purpose

QPC is intended to let an agent on one chain request a policy-governed payment to an agent on another chain. The route is coordinated by an agentic router and chain-specific adapters. This is bridge-like in outcome (value is delivered on another network), but it is not a claim that QPC is a trustless bridge, atomic swap, or universal interoperability protocol.

The governing rule is:

> Agents propose and coordinate. QPC policy authorizes. Chain adapters execute and verify. No agent can expand its own authority.

## Example: NEAR agent pays a Zcash agent

1. Agent A creates a payment intent: source identity, destination chain, asset, amount, expiry, nonce, payment ID, and requested privacy class.
2. NEAR QPC checks the agent's policy, action target, method, destination chain, argument hash, attached-deposit cap, expiry, and nonce.
3. The agentic router obtains a route quote and declares fees, trust assumptions, metadata observers, and refund/compensation behavior.
4. Agent B provides a Zcash Unified Address privately. The Zcash adapter validates network and supported receivers, then maps it to an opaque recipient reference. The raw UA must not be put in NEAR events or public route payloads.
5. The Zcash adapter uses a shielded receiver (Orchard preferred where supported; Sapling only if explicitly permitted). Transparent-only destinations fail closed by default.
6. The wallet backend constructs, signs, and broadcasts the Zcash transaction only after explicit policy and wallet authorization.
7. The adapter waits for its documented confirmation and wallet-sync criteria. A route-provider HTTP success or txid alone is not a verified shielded receipt.
8. A separately verified receipt is reconciled with the intent. Only then may the QPC workflow mark the payment completed.

## Reverse direction: Zcash agent pays a NEAR agent

1. The Zcash-side wallet adapter observes a shielded payment intended for an opaque payment ID.
2. The adapter verifies wallet ownership/receipt and the configured confirmation policy.
3. The agentic router submits a receipt claim to the NEAR-side verifier/escrow boundary.
4. NEAR releases or credits the destination only after the configured proof/trust policy is satisfied.
5. Reorgs, missing wallet data, and ambiguous receipts remain pending reconciliation; they must never be converted to success by a timeout or a provider callback.

The reverse path requires a separately reviewed Zcash receipt verifier and NEAR escrow/release design. A hosted wallet report is a provider-trust model, not a trustless proof.

## Why this differs from a conventional bridge

- **Agent-directed intent:** sender and recipient agents negotiate a payment request, quote, deadline, and route.
- **Policy-bound authority:** QPC restricts each agent's destination chain, adapter target, method, argument hash, asset/amount limits, nonce, and expiry.
- **Chain-specific execution:** NEAR uses NEAR contract calls; Zcash uses a UTXO wallet/transaction adapter. Zcash is not treated as an EVM smart-contract chain.
- **Privacy-aware routing:** shielded-only policy can reject routes that require transparent settlement. The system must disclose unavoidable source-chain, solver, timing, amount, and network metadata.
- **Verifiable completion:** intent authorization, source settlement, destination broadcast, destination confirmation, and receipt verification are distinct states.

## Required cross-chain state machine

`CREATED -> POLICY_APPROVED -> ROUTE_QUOTED -> DESTINATION_VALIDATED
-> SOURCE_SETTLED -> DESTINATION_BROADCAST -> DESTINATION_CONFIRMED
-> RECEIPT_VERIFIED -> COMPLETED`

Recoverable uncertainty enters `PENDING_RECONCILIATION`. Terminal failures use explicit `FAILED`, `EXPIRED`, or `REFUNDED` states. Every transition is idempotent and keyed by a globally unique payment ID plus chain-specific replay domain.

Do not promise cross-chain atomicity. Refunds and compensation are route-specific and must be specified before enabling real transfers.

## Test workflow

`.github/workflows/zcash-adapter.yml` is expanded into a cross-chain CI gate that runs:

- Agent-router Node tests for canonical intent hashes, policy argument binding, and route receipt verification.
- Zcash Rust adapter tests for Unified Address validation, shielded receiver policy, redaction, payment state transitions, inbound/outbound direction, replay, and reconciliation.
- A final acceptance job that fails unless all three layers pass.

The repository has separate CI workflows: .github/workflows/near-contract.yml formats, tests, and builds the NEAR Rust contract to WASM; .github/workflows/zcash-adapter.yml tests the Node router and Zcash Rust adapter. The NEAR workflow does not deploy the contract. These local tests do not broadcast testnet transactions, prove Zcash consensus independently, or establish a live bridge.

## Security invariants

- No seed phrase, spending key, or viewing key enters an agent prompt, NEAR contract state, route payload, event, or CI log.
- The NEAR side carries only an opaque recipient reference, never the raw Unified Address.
- No transparent fallback occurs unless a separate explicit policy authorizes it.
- The same payment ID, source nonce, and route receipt cannot be replayed across chains or directions.
- Agent model output is untrusted input; deterministic contract and adapter validation decide whether an action is allowed.
- Adapter/provider compromise is part of the threat model. A provider callback is not sufficient proof of settlement.
- No post-quantum or end-to-end privacy claim until implemented and independently reviewed.

## Current implementation boundary

The repository now contains a NEAR Rust policy-contract MVP, agent-router request preparation, and Zcash wallet-adapter/state-machine prototypes. The NEAR contract authorizes and records an intent only; it does not call a router or move funds. It has not been deployed to NEAR Testnet or independently audited. The repository still does **not** contain a production Zcash transaction signer/broadcaster, a trust-minimized shielded receipt proof, a live solver/bridge, atomic cross-chain settlement, or a production refund mechanism.
