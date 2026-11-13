# QPC — Quantum Persistence Contract: Architecture

## 1. QPC vision and implementation boundary

QPC (Quantum Persistence Contract) is the long-term vision for a privacy-oriented, security-focused on-chain framework for agentic finance. The first intended ecosystem target is NEAR. The architecture is intended to expand to additional Layer 1 networks through chain-specific implementations and adapters rather than assuming one contract is portable to every chain.

The intended separation is:

- **Human owner:** defines and controls authority.
- **Agent:** submits requests within the granted policy.
- **QPC enforcement layer:** validates authorization and policy before execution.
- **Chain/payment adapter:** connects the policy decision to a specific network and payment rail.

The agent must not receive the owner's root key or be able to expand its own authority.

**Current implementation boundary:** this repository contains an EVM/Solidity prototype and a first NEAR-native Rust policy-gate source prototype. The NEAR source authorizes and accounts for payment intents but does not transfer assets; build and test verification are pending. Neither implementation is a private/shielded payment system, end-to-end x402 adapter, universal multi-L1 deployment, or post-quantum cryptographic implementation. Those remain roadmap objectives.

## 2. Current implementation status

| Capability | Status | On-chain implementation |
|---|---|---|
| Owner-controlled agent configuration | Implemented | `configureAgent` |
| Agent expiry (maximum policy duration 30 days) | Implemented | `MAX_POLICY_DURATION`, `onlyActiveAgent` |
| Agent revocation | Implemented | `revokeAgent` |
| Native-asset per-transaction and cumulative limits | Implemented | `execute`, `AgentPolicy` |
| Contract target + function-selector allowlist | Implemented | `setCallPermission`, `allowedCalls` |
| ERC-20 per-transaction and cumulative limits | Implemented | `setTokenPolicy`, `transferToken` |
| ERC-20 recipient allowlist | Implemented | `setRecipientPermission`, `allowedRecipients` |
| Global execution pause | Implemented | `setPaused`, `onlyActiveAgent` |
| Owner native-asset withdrawal | Implemented | `withdrawNative` |
| Two-step ownership transfer | Implemented | `transferOwnership`, `acceptOwnership` |
| Reentrancy protection | Implemented | `nonReentrant` |
| Global execution nonce and execution events | Implemented | `nonce`, `AgentCall`, `AgentTokenTransfer` |
| DEX, lending, and payment adapters | Not implemented | Future integration work |
| Frontend, API, and visual policy builder | Not implemented | Future application work |
| Separate settlement engine | Not implemented | No standalone settlement module exists |
| NEAR-native contract implementation | Not implemented | First ecosystem target; requires a separate NEAR implementation |
| Private/shielded payment rail | Not implemented | Requires chain- and wallet-specific privacy design |
| x402 payment adapter | Not implemented | x402 does not itself provide payment privacy |
| Additional L1 implementations | Not implemented | Requires a separate adapter and security review per chain |
| Post-quantum signatures/key verification | Not implemented | No ML-DSA, SLH-DSA, or other PQ signature verification in this code |
| Cross-chain bridge or cross-chain agent payments | Not implemented | Requires separately designed and audited interoperability protocol |

The repository currently contains a core contract, Foundry tests, and a deployment script. It does not contain the future frontend/API/adapters shown later in this document.

## 3. High-level architecture — implemented core

```
Owner
  |
  | configure / pause / revoke
  v
QPCAgentFinance
  |
  | agent request
  v
Policy checks
  |-- agent active and not expired
  |-- contract not paused
  |-- target + selector allowed (native call)
  |-- recipient allowed (token transfer)
  |-- per-transaction limit
  |-- cumulative spending cap
  |
  +--> reject (revert)
  |
  +--> account spending + increment nonce
          |
          +--> target contract call with native value
          |
          +--> ERC-20 transfer
          |
          +--> execution event
```

The native-call and ERC-20-transfer paths are distinct entry points: `execute` and `transferToken`.

## 4. Contract responsibilities

### `QPCAgentFinance.sol`

The contract stores and enforces:

- owner and pending-owner state
- agent activation flag and expiry
- native spending cap, spent amount, and per-transaction limit
- per-agent/per-token cap, spent amount, and per-transaction limit
- agent/target/function-selector permissions
- agent/recipient permissions
- global pause state
- global execution nonce
- execution and administration events

The contract does not generate an investment strategy, select a DEX, calculate lending yield, or decide which protocol an agent should use.

### Agent

An agent can:

- call `execute` for a permitted target and selector, subject to native-asset limits
- call `transferToken` for a configured token and permitted recipient, subject to token limits

An agent cannot:

- configure or increase its own policy
- grant itself permissions
- revoke or unpause itself
- change the owner
- bypass expiry, pause, allowlists, or spending limits

### Owner

The owner can:

- configure an inactive agent
- revoke an agent
- set target/selector permissions
- set recipient permissions
- set token policies
- pause or unpause agent execution
- withdraw native assets
- start and accept ownership transfer through the two-step ownership process

Policy changes that reset spending accounting are restricted while the agent is active. An expired policy remains marked active in storage until it is revoked; revoke it before configuring that agent again.

## 5. Guardrail pipeline

### Native-asset contract call

```
Agent calls execute(target, value, data)
  |
  v
Agent active and not expired?
  | no -> revert
  v
Contract paused?
  | yes -> revert
  v
Target and selector permitted?
  | no -> revert
  v
Per-transaction limit satisfied?
  | no -> revert
  v
Cumulative native cap and balance satisfied?
  | no -> revert
  v
Update spending accounting and nonce
  |
  v
External call
  | failure -> revert entire transaction
  v
Emit AgentCall
```

Accounting occurs before the external call. If the external call reverts, EVM transaction atomicity also reverts the accounting and nonce changes.

### ERC-20 transfer

```
Agent calls transferToken(token, recipient, amount)
  |
  v
Agent active, unexpired, and contract unpaused?
  | no -> revert
  v
Recipient permitted?
  | no -> revert
  v
Token per-transaction and cumulative limits satisfied?
  | no -> revert
  v
Update token accounting and nonce
  |
  v
Call token transfer
  | failed / explicit false -> revert
  v
Emit AgentTokenTransfer
```

The transfer handler accepts standard boolean-returning ERC-20 tokens and tokens that return no data. A failed low-level call or an explicit false return is rejected. Token compatibility still depends on the behavior of the token contract.

## 6. Spending policies and lifecycle details

- Native policy duration is capped at 30 days.
- Native and token policies each have a cumulative cap and per-transaction limit.
- A token policy is configured per agent and token address.
- Token transfers additionally require a recipient allowlist entry.
- Contract calls require an exact agent + target + four-byte selector permission.
- An agent's expiry blocks execution when `block.timestamp >= expiresAt`.
- Expiry does not itself write `active = false`; the owner must revoke the agent before reconfiguring it.
- The nonce is global to the contract and increments for successful native calls and token transfers.

## 7. Emergency and deposit behavior

The owner can pause agent execution globally. While paused, `execute` and `transferToken` are blocked.

There are two native-asset deposit paths in the current contract:

- `deposit()` checks the pause state and reverts while paused.
- The payable `receive()` function accepts direct native transfers and is **not** pause-gated.

Therefore, pausing currently stops the guarded deposit function and agent execution, but does not stop direct transfers to the contract address. This is intentional documentation of the current code behavior, not a claim that all deposits are blocked.

Owner withdrawal through `withdrawNative` remains available while paused, so the owner can recover native assets during an emergency. ERC-20 assets are not covered by this native withdrawal function.

## 8. Settlement and audit trail

QPC does not implement a separate settlement engine.

For native execution, the contract forwards the requested native value as part of the permitted target call. For token execution, it calls the configured token's `transfer` function. Successful operations emit execution events and increment the global nonce.

Events and the nonce provide on-chain traceability, but they are not by themselves a complete off-chain audit product, transaction indexer, accounting service, or proof that an external protocol performed the intended economic action.

## 8A. Confidential AI inference integration — optional

NEAR AI Cloud may provide confidential inference for QPC's agent reasoning or private policy-assistance workload. This is an **off-chain compute service**, not a smart-contract runtime or payment rail.

- Use a model explicitly classified as Private TEE when the policy requires confidential inference.
- Verify fresh gateway/model attestation and response signatures with the supported NEAR AI SDK/verifier. Bind evidence to a fresh nonce, expected model, and request.
- Treat LLM output as an untrusted proposal. The deterministic QPC policy evaluator and on-chain authorization remain the sole authority for payment/action permission.
- Never provide wallet seed phrases, signing keys, or raw secrets to the model.
- Do not treat Incognito/external-provider models as equivalent to Private TEE models.
- Attestation validates evidence about a specific measured environment; it does not prove model correctness, bug absence, or application safety.
- TEE inference does not make on-chain NEAR transfers private and does not replace a shielded payment rail.

The NEAR AI Gateway desktop app is a developer tool for routing supported local coding-agent sessions through NEAR AI Cloud. A deployed QPC product should call the Cloud API/SDK directly; it does not need the macOS app as a runtime dependency.


## 8B. NEAR-native policy-gate source prototype

A first Rust contract prototype now lives at `near/qpc-policy/`, using NEAR SDK 5.30.1.

Implemented in source: owner-only policy configuration, 30-day maximum expiry, per-payment and cumulative authorization caps, recipient allowlist (maximum 64 unique accounts), pause, revocation, policy versions, sequential per-agent nonces, request-hash format validation, and public audit logs.

**Critical limitation:** `authorize_payment` only records authorization and reserves the amount against the policy cap. It does not transfer NEAR or FT assets, call a merchant, verify x402 settlement, or make transaction data private. No settlement/cancel lifecycle exists yet. It must not be used for real funds.

Unit-test cases and a GitHub Actions workflow for Rust tests and WASM compile checking are included. Their success remains unverified until CI completes.


## 8C. Cybersecurity defense-in-depth

The NEAR source prototype includes owner-only administration, two-step ownership transfer, a pause-only emergency guardian, bounded policy duration and recipient list, a lifetime cap of 256 unique agent policies, sequential nonces, checked arithmetic, and a contract-derived SHA-256 intent digest. The digest currently binds deployment domain, contract account, agent, policy version, nonce, recipient, and amount; it does not yet bind asset, service, network, or settlement expiry.

The owner account is a critical trust root. Production ownership should use a properly configured NEAR multisig. The guardian can pause but cannot unpause or alter policies. These controls reduce risk but do not make the contract immune to attacks.

Detailed threat model and release gate: [QPC NEAR Policy Gate Security Model](docs/QPC_NEAR_POLICY_SECURITY.md).


## 9. QPC roadmap — planned, not implemented

The following describes the intended product direction. None of these items should be presented as an active feature until implementation and verification are available.

### 9.1 NEAR-first private-contract direction

NEAR is the first intended ecosystem target. The current EVM/Solidity contract cannot simply be copied to NEAR: a NEAR-native contract and its authorization, storage, execution, and privacy model must be designed and tested for that environment.

Privacy must be specified precisely. The design must identify which transaction fields, balances, identities, metadata, and application state are hidden; which remain public; who can decrypt or prove information; and how recovery and compliance requirements are handled. Do not describe ordinary public on-chain state as private.

### 9.2 Multi-L1 architecture

QPC aims to support multiple L1 networks through chain-specific contract implementations and adapters. Each network has different execution models, account/key systems, finality, fee handling, privacy properties, and verification capabilities. There is no assumption that one Solidity contract can run unchanged on every L1.

### 9.3 Private agent payments and x402

The planned payment layer includes agent-to-agent and agent-to-service payments, potentially using x402 for HTTP payment negotiation and payment requirements.

x402 is a payment protocol/interface, not a privacy guarantee. A private x402 flow would require a supported private settlement rail and an adapter that binds the x402 request to the correct chain, asset, recipient, amount, expiry, and replay-protection context. The adapter must reject unsupported privacy modes rather than silently falling back to transparent settlement. The exact privacy guarantees and metadata exposure must be documented for each supported rail.

### 9.4 Security engineering

QPC's security objective is defense in depth against known attack classes and implementation vulnerabilities. Planned work includes threat modelling, authorization and key-management review, replay protection, safe external-call patterns, fuzzing, invariant/property testing, static analysis, dependency review, incident response, and independent audits.

No smart contract can honestly be described as immune to hackers or bugs. Security claims must be scoped to specific controls, tests, audits, and known limitations.

### 9.5 Post-quantum readiness

Quantum resilience is a research and implementation objective only. It requires selecting standardized post-quantum cryptographic algorithms, evaluating chain-native verification support and gas/cost constraints, defining key rotation and migration, and considering hybrid transition paths. The current Solidity contract does not implement ML-DSA, SLH-DSA, or any other post-quantum signature verification. QPC must not be marketed as quantum-resistant until the relevant implementation has been independently reviewed and tested.

### 9.6 Conceptual future architecture

```
Human / Agent
      |
      v
Policy + Authorization Layer
      |
      v
QPC chain-specific implementation
      |
      +--> NEAR contract and privacy design       [PLANNED FIRST]
      +--> EVM-compatible L1 adapter               [CURRENT PROTOTYPE ONLY]
      +--> Other L1 adapters                       [PLANNED]
      |
      v
Private payment integration                       [PLANNED]
      |
      +--> Agent-to-agent payments
      +--> Agent-to-service payments
      +--> x402 negotiation/payment adapter
      |
      v
Events, proofs, and audit tooling                  [PLANNED EXTENSIONS]
```

Each chain, privacy rail, x402 integration, and cryptographic component needs its own threat model, tests, and review. Support on one network does not imply support or equivalent privacy/security on another.

## 10. Test architecture

The repository separates security, invariant, and stateful testing concerns. The presence of test files or test scaffolding does not prove that every property has been exhaustively verified.

Expected validation sequence:

1. Build with the configured Solidity compiler.
2. Run unit and security regression tests.
3. Run invariant tests.
4. Run stateful handler tests.
5. Run static analysis and review findings.
6. Obtain independent security review before production use.

A passing test suite is not equivalent to an independent security audit. Test results must be reported from the actual CI/local run for the commit being evaluated.

## 11. Security principles

1. Least-privilege agent authority.
2. Human-controlled policy ownership.
3. Deterministic on-chain enforcement.
4. Fail-closed permission and spending checks.
5. Short-lived agent authority.
6. Explicit target, selector, and recipient allowlists.
7. Bounded cumulative and per-transaction spending.
8. Emergency pause for agent execution.
9. No AI custody of the owner's private key.
10. Keep the core contract small and protocol-agnostic.
11. Label planned integrations as planned until implemented and tested.

## 12. Intended user flow

The future application experience may be:

**Intent → Policy draft → Human review/approval → Agent request → QPC enforcement → On-chain call/transfer → Event-based audit**

Only the QPC enforcement and on-chain call/transfer portions are implemented by the current contract. The UI, policy drafting workflow, agent orchestration, and off-chain audit interface remain application-layer work.
