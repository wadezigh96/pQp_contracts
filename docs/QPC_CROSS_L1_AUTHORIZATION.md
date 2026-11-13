# QPC Cross-L1 Authorization Envelope — Design Specification

**Status: proposed protocol specification; not implemented as a complete cross-L1 envelope.**

This document defines the target authorization boundary for QPC. It is deliberately separate from any one chain's smart contract. Chain adapters must enforce the envelope using their own native authorization, transaction, finality, and proof rules.

## 1. Security objective

One authorization must describe one bounded action and must not be reusable for another agent, source chain, destination chain, asset, amount, recipient, policy version, or time window.

The envelope is an authorization commitment, not a bridge, signature scheme, privacy proof, or settlement receipt by itself.

## 2. Required authorization fields

A future versioned envelope MUST bind all of the following:

| Field | Required meaning |
|---|---|
| `schema` | Protocol name and explicit envelope version |
| `intent_id` | Globally unique, non-reusable authorization identifier |
| `agent_identity` | Agent identity namespace, identifier, and controlling key/account reference |
| `source` | Source network identifier, source account/agent account, and source adapter |
| `destination` | Destination network identifier, destination account or privacy-preserving recipient commitment, and destination adapter |
| `action` | Fixed action type and exact permitted operation/parameter set |
| `asset` | Source and destination chain-specific asset identifiers, decimals, and explicit mapping |
| `amount` | Canonical integer base-unit amount; no floating-point values |
| `fees` | Maximum fee and fee asset, plus any permitted conversion/slippage bounds |
| `recipient` | Exact public recipient or domain-separated privacy commitment; raw shielded address material must not be published |
| `nonce` | Monotonic or unique nonce within an explicit replay domain |
| `replay_domain` | Source chain, source contract/program/account, adapter, and authorization scope |
| `policy` | Policy identifier, immutable/versioned policy version, and policy commitment |
| `time` | Issued-at time, expiry/deadline, and any not-before constraint |
| `settlement` | Required destination state, proof/evidence class, confirmation/finality policy, and timeout/reconciliation rules |
| `cancellation` | Revocation/cancellation authority and how adapters learn and enforce cancellation |

No field may be silently defaulted, inferred from an RPC response, or reinterpreted by a destination adapter. Asset decimals and network identifiers must be explicit.

## 3. Canonical commitment and authentication

The implementation MUST define a single deterministic, versioned serialization profile before multiple adapters interoperate. It must specify:

1. field names/tags, types, required/optional fields, and rejection of unknown critical fields;
2. canonical byte ordering and integer representation;
3. UTF-8 and string normalization rules;
4. domain separation including protocol, envelope version, and action type;
5. a cryptographic commitment over the complete authorization envelope;
6. an authentication/signature profile and key/identity binding;
7. algorithm identifiers and downgrade rejection.

A hash only commits to bytes; it does **not** authenticate the owner or agent, conceal the content, or prove settlement. Authentication must be checked against the authorized identity and active policy. Post-quantum or hybrid authentication remains unavailable until separately implemented and tested.

## 4. Replay and mutation resistance

Every adapter MUST reject an envelope if:

- its intent ID or nonce has already been consumed in the relevant replay domain;
- source/destination network, adapter, account, asset, amount, fee bound, recipient commitment, action, or policy version differs from the authenticated commitment;
- it is not yet valid, expired, cancelled, or revoked;
- the agent identity or key is not authorized by the referenced policy;
- the policy is stale or the required spending limit would be exceeded;
- the settlement evidence is for a different intent, chain, asset, amount, or recipient commitment.

Replay state must survive policy revocation/reactivation and must not reset when a policy version changes. Cross-chain replay domains must not be shared accidentally between unrelated chains or adapters.

## 5. Settlement evidence is a separate object

Authorization is not settlement. After execution, a separate evidence object MUST reference the exact authorization commitment and include, as applicable:

- destination network and adapter;
- transaction/signature identifier;
- block/slot/height and inclusion location;
- confirmations or chain-specific finality status;
- verifier identity and verification method;
- proof or independently obtained chain data required by that adapter;
- recipient/amount match result under the chain's privacy model;
- observation time and reorg/reconciliation status.

A transaction hash, RPC response, relayer callback, or provider HTTP success alone is not proof of settlement. A verifier must check evidence against the correct chain and required trust model. Shielded Zcash outputs cannot be treated as publicly readable recipient/amount fields; verification needs an appropriate wallet/viewing or cryptographic proof model without disclosing viewing keys.

## 6. Adapter-specific enforcement

| Adapter | Required native validation |
|---|---|
| NEAR | account and method access, attached deposit, receipt chain, nonce, block/finality and contract state |
| Ethereum | chain ID, account nonce, EVM target/value/calldata, token semantics, receipt and finality |
| BSC | BSC chain ID and BSC-specific RPC/finality assumptions, plus EVM call and token validation |
| Solana | cluster/genesis identity, program IDs, instruction/account constraints, signer/PDA rules, blockhash/nonce and commitment |
| Zcash | network/consensus identity, Unified Address receiver policy, shielded note/nullifier handling, wallet scan state and shielded receipt verification |

A common envelope does not make the adapters interchangeable. Each adapter owns its serialization boundary, transaction construction, signer isolation, finality model, error handling, and reconciliation.

## 7. Privacy requirements

- Keep seeds, spending keys, and viewing keys out of agent prompts, contracts, logs, CI, and router payloads.
- Use recipient commitments or private channels only where their threat model and key ownership are explicit.
- Minimize public identifiers and correlate them only when required.
- Treat timing, amount, fees, source transactions, relayers, and network metadata as possible leakage.
- Do not call a hash, opaque reference, or encrypted off-chain message “private on-chain” by itself.
- Fail closed when a route cannot satisfy the declared privacy class; never silently downgrade shielded settlement to transparent settlement.

## 8. Failure and recovery

The cross-chain state machine must be idempotent and explicit:

`CREATED -> AUTHORIZED -> ROUTE_ACCEPTED -> SOURCE_CONFIRMED -> DESTINATION_SUBMITTED -> DESTINATION_CONFIRMED -> EVIDENCE_VERIFIED -> COMPLETED`

Uncertain states enter `PENDING_RECONCILIATION`. Cancellation, expiry, reorg, partial completion, duplicate delivery, timeout, and refund/compensation must be explicit transitions with adapter-specific rules. Do not claim atomicity unless the protocol actually provides it.

## 9. Implementation status (2026-11-13)

The agent-router now contains an initial, versioned QPC v1 envelope implementation in `agent-router/src/cross-l1-envelope.mjs`:

- strict field allowlists and rejection of unknown fields;
- deterministic canonical serialization and SHA-256 commitment over the complete envelope;
- explicit binding of agent/key identity, source/destination networks and adapters, assets/decimals, amounts, fee bounds, recipient commitment, nonce/replay domain, policy, time window, settlement policy, and cancellation epoch;
- an authorization boundary that requires caller-supplied identity authentication and durable atomic replay-consumption callbacks;
- a separate settlement-evidence boundary that checks the authorization commitment and expected destination fields before invoking a chain-specific evidence verifier.

The new module is not yet wired into the existing NEAR dispatch path or a production chain adapter. The verifier callbacks are dependency boundaries, not built-in signature verification or consensus proofs. The replay callback must be backed by durable atomic storage supplied by an adapter; the library itself does not persist replay state. The current NEAR contract still uses its narrower NEAR-to-Zcash policy-argument hash and records policy authorization only. It does not execute a transfer or independently verify Zcash settlement.

No production authentication profile, post-quantum signature, hybrid key scheme, ZK proof, or live cross-L1 settlement is activated by this module. The v1 schema is an initial protocol baseline and must receive independent review, adapter-specific integration, and CI/security testing before production use.

## 10. Required acceptance tests before activation

Each adapter must test at minimum:

- one-field mutation for every authorization field;
- wrong source chain, destination chain, adapter, account, asset, decimals, amount, fee, recipient, action, policy ID/version, nonce, and expiry;
- signature/key mismatch, revoked key, stale policy, and algorithm downgrade;
- duplicate intent, duplicate nonce, and replay across chain/direction/adapter;
- forged provider receipt, wrong transaction, wrong block/slot, insufficient finality, and reorg;
- privacy downgrade and transparent-only destination rejection;
- timeout, cancellation, partial completion, retry idempotency, and refund/reconciliation;
- malformed encodings, non-canonical integers, unknown critical fields, and resource-exhaustion inputs.

Passing unit tests is not an independent security or cryptographic audit.
