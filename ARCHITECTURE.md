# QPC Agent Finance — Architecture

## 1. Purpose and implementation boundary

QPC Agent Finance is a smart-contract policy enforcement layer for agent-controlled execution.

Its core separation is:

- **Human owner:** defines and controls authority.
- **Agent:** submits requests within the granted policy.
- **QPC contract:** deterministically checks and enforces the on-chain policy.

The agent does not receive the owner's private key and cannot change its own policy.

This document distinguishes **implemented contract behavior** from **planned application-layer work**. A diagram or future integration listed here is not evidence that the feature is already implemented.

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
| Cross-chain bridge or cross-chain agent payments | Not implemented | Not part of this contract |

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

## 9. Planned application layer — not implemented

The following is a possible future architecture, not a current repository feature:

```
User
 |
 v
Frontend / API                 [PLANNED]
 |
 v
Policy Builder                 [PLANNED]
 |
 v
Agent service                  [PLANNED]
 |
 v
QPCAgentFinance                [IMPLEMENTED CORE]
 |
 +--> DEX adapter              [PLANNED]
 +--> Lending adapter          [PLANNED]
 +--> Payment adapter          [PLANNED]
 +--> Other protocol adapters  [PLANNED]
```

Adapters would translate an application's intent into specific target calls and selectors. They must not bypass QPC policy checks. Each adapter requires its own threat model, allowlist design, tests, and review before it can be described as supported.

The current contract is protocol-agnostic: it does not natively integrate with a specific DEX, lending market, payment provider, chain bridge, or wallet service.

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
