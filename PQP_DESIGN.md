# PQP — Implementation Plan and Decision Record

Status: Approved project direction; implementation in progress.
Branch: pqp/near-core-multil1.
Original base: main at 76e23e1a50a6d23f188c6954c07bb78d8c28fa9e.

## Product definition

PQP means Post-Quantum Persistence. It is a multi-L1 security framework with deterministic on-chain policy enforcement, constrained AI/agent execution, crypto-agile identity and authorization, post-quantum migration support where native chain capabilities permit, and monitoring, incident response and recovery controls.

PQP is not a universal anti-hack switch and does not modify L1 consensus simply by deploying an application contract.

## Architecture decision

One chain-neutral policy specification, multiple native adapters.

Rejected:
- One identical contract claimed to work across every L1.
- AI/monitor output used as sole transaction authorization.
- Generic target + selector allowlisting treated as sufficient call safety.
- Custom post-quantum algorithms.
- Claiming ML-DSA support at application layer without verifying chain runtime and wallet path.

## NEAR-first scope

### Milestone 0 — inherited EVM prototype audit
Preserve the EVM prototype as a separate audit input. Record CI blockers and security findings. Do not carry unsafe generic execution semantics into the NEAR core.

### Milestone 1 — NEAR policy core
Implemented initial vertical slice in `near/pqp-near`:
- Rust workspace pinned to NEAR SDK 5.30.1.
- Owner/governance-controlled policy lifecycle.
- Immutable agent identity per policy.
- Policy version, validity window and revocation.
- Per-action and cumulative deposit limits.
- Exact target, method and argument-digest constraints.
- Sequential nonce and request-ID replay resistance.
- Unit tests for authorization, unauthorized callers, replay, argument substitution, budget limits, pause and expiry.

The authorization ledger does not yet dispatch external calls or transfer assets. That is an explicit safety boundary, not a completed execution feature.

### Milestone 2 — constrained execution
- NEAR-native cross-contract call adapter.
- Explicit dispatched, succeeded and failed states.
- Authenticated, idempotent callbacks.
- Method-specific argument checks.
- Storage and attached-deposit accounting.
- Testnet integration and measured gas.

### Milestone 3 — post-quantum integration
- Verify actual NEAR account ML-DSA availability on target network.
- Verify key type, wallet signing, transaction encoding and SDK behavior.
- Keep application-level signature verification separate from native transaction authentication.
- Add algorithm/key version metadata and rotation tests.
- No private-key storage in contracts.
- No claim NEAR consensus is fully post-quantum unless the chain itself confirms it.

### Milestone 4 — multi-L1 adapters
- EVM: Solidity adapter with typed actions, safe token handling and signature capability review.
- Solana: Rust program with signer, PDA, account ownership and instruction constraints.
- Cross-chain: explicit message IDs, source/destination domains, replay protection and finality assumptions.
- Shared policy conformance test vectors.

## Current API surface

- `create_policy(policy_id, input)`
- `update_policy(policy_id, expected_version, input)`
- `revoke_policy(policy_id)`
- `set_paused(policy_id, paused)`
- `dispatch_action(request)`\n- `on_action_result(policy_id, request_id, nonce)` (private callback)
- `get_policy(policy_id)`
- `get_usage(policy_id)`
- `get_action_usage(policy_id, action_id)`

`dispatch_action` executes one constrained target call through a NEAR promise and private callback. This is a prototype and not yet approved for production assets.

## Invariants

1. Only policy authority can create or change policy.
2. An agent cannot expand its own permission.
3. Every request is bound to a live policy version.
4. Expired, revoked or paused policy cannot authorize an action.
5. Usage cannot exceed per-call, per-action or cumulative budgets.
6. Duplicate request IDs/nonces cannot authorize twice.
7. External call failure cannot silently count as successful settlement once execution is added.
8. Callback execution cannot be forged or replayed once execution is added.
9. Monitoring cannot move assets or expand permissions.
10. Key rotation preserves explicit recovery and replay boundaries.

## Validation plan

Unit tests for all state transitions. Property tests for budget monotonicity, expiry and nonce uniqueness. Adversarial tests for malicious target arguments and forged callbacks. Integration tests against local NEAR runtime and testnet. Crypto tests using standardized vectors and supported implementations. Cross-adapter conformance tests. Independent review before mainnet asset custody.

## Scope guardrails

Main remains unchanged. No deployment, wallet connection, transaction, token movement or hackathon submission. No external paid API/model use without explicit cost approval. No secrets or private keys in repository. No unhackable, quantum-proof or fully post-quantum marketing claims without evidence.