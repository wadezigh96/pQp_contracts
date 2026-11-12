# PQP Architecture

## 1. Mission and bounded security claim

PQP (Post-Quantum Persistence) is a modular security framework for on-chain assets and agentic execution across Layer-1 networks.

PQP can enforce explicit policy in a contract or native program, integrate post-quantum account/signature capabilities where the host chain supports them, and provide monitoring and recovery controls. It cannot guarantee that an L1, bridge, wallet, oracle, RPC, external protocol or operator is immune to compromise.

Human/governance is the authority. The agent is a constrained requester. The PQP adapter is the deterministic policy enforcement point.

## 2. Layered architecture

Human / Governance
  -> Chain-neutral PQP Policy Specification
  -> NEAR Adapter (Rust) | EVM Adapter (Solidity) | Solana Adapter (Rust)
  -> Native execution and chain-local policy state
  -> Events and audit data
  -> Off-chain PQP Monitor (alerts; no implicit custody)

The policy specification is shared. Execution code, signature verification, account permissions, finality assumptions and storage models remain chain-specific.

## 3. Policy model

Every policy should define:
- stable policy ID scoped by chain and contract;
- monotonically increasing policy version;
- owner/governance authority and agent identity;
- valid-after and expiry;
- paused and revoked state;
- allowed action identifiers (target + method + canonical argument digest);
- allowed assets and destinations where applicable;
- per-action and cumulative spend/deposit limits;
- maximum attached value/deposit;
- nonce or sequence domain;
- policy update and recovery threshold.

A policy update must not reset consumed limits or replay sequence. Expiry and revocation fail closed.

## 4. Agent request pipeline

1. Authenticate the requester using the host chain's native transaction/account model.
2. Resolve the current policy and version.
3. Reject if paused, revoked, not-yet-valid or expired.
4. Validate action ID, target, method, canonical argument digest and attached value.
5. Validate asset, destination, per-action limit and cumulative budget.
6. Consume a chain-specific nonce and update accounting atomically.
7. Execute only the validated action in the execution-adapter milestone.
8. Record result status and emit an audit event.

## 5. NEAR-native implementation

The implementation is in `near/pqp-near` and uses the NEAR Rust SDK. It provides a policy registry and bounded promise execution:
- owner-only create/update/pause/revoke;
- immutable agent identity;
- version-checked updates;
- 90-day maximum policy duration;
- exact target/method/argument-digest rules;
- per-call, per-action and policy-wide deposit accounting;
- sequential nonce and unique request-ID enforcement;
- fail-closed handling of paused, revoked, inactive and expired policies.

The initial `authorize_action` method records a permit and consumes budget. It intentionally does not dispatch a promise or move assets. This prevents the first security milestone from introducing an unreviewed arbitrary-call executor.

Next milestone:
- NEAR-native cross-contract promise adapter;
- explicit dispatched, succeeded and failed states;
- authenticated, idempotent callbacks;
- callback replay protection;
- storage and attached-deposit accounting;
- local sandbox integration and measured gas.

NEAR Function Call Access Keys are an additional least-privilege control, not a replacement for contract policy checks.

## 6. Post-quantum boundary

NEAR's published roadmap and SDK release notes describe ML-DSA account-signing and verification capabilities, but PQP must verify exact mainnet/testnet support, SDK serialization, key type, wallet support, fees and transaction compatibility before advertising a capability.

Application-level signature verification is separate from native transaction authentication. Do not assume a Rust contract can cheaply or natively verify a signature unless the runtime exposes a supported host function/precompile or a reviewed verifier is integrated and benchmarked.

## 7. Crypto agility

Use standardized algorithms; do not design custom cryptography.

| Standard | Correct role |
|---|---|
| FIPS 204 ML-DSA | Post-quantum digital signatures |
| FIPS 205 SLH-DSA | Hash-based post-quantum digital signatures |
| FIPS 203 ML-KEM | Key establishment/encapsulation, not transaction signing |

Represent algorithm ID/version, public-key and signature encoding versions, key activation/retirement, migration status and chain-native verification status. Never store private keys in PQP contracts. Key migration requires tested recovery and replay behavior.

## 8. Threat monitoring and incident response

The on-chain contract enforces deterministic invariants. An off-chain monitor can inspect events, RPC data where available, contract changes and behavior baselines.

Monitor outputs are advisory unless a separately authorized guardian mechanism is configured. Guardian powers should be narrow (for example, pause only), threshold-controlled, rate-limited and time-bounded. A monitor must never unilaterally withdraw or redirect assets.

## 9. Multi-L1 adapter contract

Every adapter implements shared policy test vectors:
- authorization and role separation;
- expiry and revocation;
- pause behavior;
- per-action and cumulative budgets;
- destination and method constraints;
- replay/duplicate resistance;
- reentrancy or callback safety;
- failure and partial-settlement accounting;
- upgrade and key-rotation rules;
- event completeness.

Adapter order: NEAR native Rust first; EVM Solidity second; Solana Rust third; additional L1s only after runtime, cryptography, finality and recovery review.

Cross-chain messaging, bridges, relayers and MPC signers are separate threat domains. Future messages must bind source/destination domains, message ID, nonce, expiry and proof/finality state.

## 10. Validation gates

A: specification and invariants reviewed.
B: unit, property, fuzz and adversarial tests pass.
C: testnet integration, callbacks, storage and gas/compute costs measured.
D: cryptographic key formats and signing/verification paths independently validated.
E: pause, recovery, upgrade and monitoring drills completed.
F: independent audit and remediation completed before production funds.

A green CI pipeline is not an audit and is not evidence of quantum resistance by itself.