# PQP — Implementation Plan and Decision Record

Status: Approved project direction; design baseline.
Branch: pqp/near-core-multil1.
Base: main at 76e23e1a50a6d23f188c6954c07bb78d8c28fa9e.

## Product definition

PQP means Post-Quantum Persistence. It replaces POA as the product name and top-level concept.

PQP is a multi-L1 security framework with deterministic on-chain policy enforcement, constrained AI/agent execution, crypto-agile identity and authorization, post-quantum migration support where native chain capabilities permit, and monitoring, incident response and recovery controls.

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

### Milestone 0 — inherited-code audit
Preserve POA as legacy baseline. Record CI blocker and security findings. Do not carry unsafe generic execution semantics forward.

### Milestone 1 — NEAR policy core
- Rust workspace using a pinned NEAR SDK version.
- Owner/governance and agent roles.
- Policy create/update/revoke/pause.
- Policy version, expiry and nonce.
- Per-action and cumulative budget.
- Typed action model; no arbitrary call forwarding in first release.
- Events for policy and execution state changes.
- Unit and adversarial tests.

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

## Initial NEAR module API (design, not final ABI)

- create_policy(policy)
- update_policy(policy_id, expected_version, policy)
- revoke_policy(policy_id)
- set_paused(policy_id, paused)
- authorize_action(policy_id, request)
- on_action_result(request_id, result) — private/authenticated callback only
- get_policy(policy_id)
- get_agent_usage(policy_id, agent_id)

Final ABI must be reviewed against the selected NEAR SDK version and asynchronous execution semantics.

## Invariants

1. Only policy authority can create or change policy.
2. An agent cannot expand its own permission.
3. Every request is bound to a live policy version.
4. Expired, revoked or paused policy cannot dispatch an action.
5. Usage cannot exceed per-action or cumulative budgets.
6. Duplicate request IDs/nonces cannot execute twice.
7. External call failure cannot silently count as successful settlement.
8. Callback execution cannot be forged or replayed.
9. Monitoring cannot move assets or expand permissions.
10. Key rotation preserves explicit recovery and replay boundaries.

## Validation plan

Unit tests for all state transitions. Property tests for budget monotonicity, expiry and nonce uniqueness. Adversarial tests for malicious target arguments and forged callbacks. Integration tests against local NEAR runtime and testnet. Crypto tests using standardized vectors and supported implementations. Cross-adapter conformance tests. Independent review before mainnet asset custody.

## Current inherited baseline

- Latest main CI run: build fails before tests; test step skipped.
- Compiler error: payable cast in test/POAAgentFinance.stateful.t.sol:14.
- foundry.toml contains unsupported target_contracts.
- execute() validates target and selector but not dangerous calldata arguments.
- No PQC implementation exists in inherited Solidity code.
- No independent audit is documented.

## Scope guardrails

Main remains unchanged. No deployment, wallet connection, transaction, token movement or hackathon submission. No external paid API/model use without explicit cost approval. No secrets or private keys in repository. No unhackable, quantum-proof or fully post-quantum marketing claims without evidence.
