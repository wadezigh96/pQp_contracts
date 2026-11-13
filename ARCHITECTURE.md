# QPC — Legacy EVM Policy Contract Architecture

This document describes the older Solidity policy-guardrail baseline only. It is separate from the native NEAR QPC prototype and does **not** verify ML-DSA or provide post-quantum signature protection.

## 1. Design goal

QPC is a policy enforcement layer for agentic finance.

It is designed around one separation:

- **Human = authority**
- **Agent = constrained executor**
- **Contract = deterministic enforcement**

The AI does not receive the owner's private key and does not get unrestricted contract access.

## 2. High-level architecture

```
                    HUMAN
                      │
                      ▼
              ┌───────────────┐
              │ Policy Builder│
              └───────┬───────┘
                      │
          ┌───────────┴───────────┐
          ▼           ▼           ▼
       Limits      Allowlist    Expiry
          │           │           │
          └───────────┬───────────┘
                      ▼
              ┌───────────────┐
              │ QPC CONTRACT  │
              │ Enforcement   │
              └───────┬───────┘
                      │
              ┌───────┴───────┐
              ▼               ▼
        Agent Execution    Rejection
              │
              ▼
          Settlement
              │
              ▼
       Events + Nonce
```

## 3. Component responsibilities

### QPCAgentFinance.sol

The contract is the security boundary.

It owns:

- agent activation state
- expiry
- native spending accounting
- ERC-20 spending accounting
- call permissions
- recipient permissions
- pause state
- ownership lifecycle
- execution nonce
- execution events

It does not decide investment strategy.

### Agent

The agent may:

- request an allowed contract call
- request an allowed token transfer

The agent may not:

- change its own policy
- activate itself
- increase its limits
- bypass expiry
- bypass pause
- change recipients
- change call permissions

### Owner

The owner may:

- configure agents
- revoke agents
- configure call permissions
- configure recipients
- configure token policies
- pause/unpause execution
- withdraw native funds
- transfer ownership

## 4. Guardrail pipeline

Every agent execution follows:

```
REQUEST
  │
  ▼
Agent active?
  │
  ├── NO → REVERT
  │
  ▼
Expired?
  │
  ├── YES → REVERT
  │
  ▼
Paused?
  │
  ├── YES → REVERT
  │
  ▼
Permission check
  │
  ├── FAIL → REVERT
  │
  ▼
Spending check
  │
  ├── FAIL → REVERT
  │
  ▼
Accounting
  │
  ▼
External call
  │
  ▼
Event + nonce
```

## 5. Native execution

Native execution requires:

- active agent
- valid expiry
- unpaused contract
- target allowlist
- selector allowlist
- amount <= per-transaction limit
- cumulative amount <= policy cap
- sufficient contract balance

Accounting is updated before the external call. If the call reverts, the entire transaction reverts, including the accounting update.

## 6. Token execution

Token execution requires:

- active agent
- valid expiry
- unpaused contract
- recipient allowlist
- configured token policy
- amount <= per-transaction limit
- cumulative amount <= token cap

The low-level ERC-20 call accepts both standard boolean-returning tokens and tokens that return no data, while rejecting failed calls and explicit false returns.

The token address must contain contract code. Generic `execute` calls reject standard token outflow and allowance selectors (including ERC-20 transfer/approval selectors, `approveAndCall`, and common callback-transfer selectors) so those direct token actions cannot bypass token caps or recipient allowlists. Other owner-approved protocol calls remain trusted capabilities; this contract cannot generically infer every asset effect of arbitrary protocol calldata.

## 7. Emergency model

The owner can pause execution globally.

When paused:

- agent execution stops
- token transfers through the agent stop
- calls to the guarded `deposit()` function stop

The payable `receive()` function continues to accept direct native-asset transfers. Forced ETH transfers can also bypass a `receive()` guard. Pause therefore stops execution, not all possible balance changes. Owner withdrawal remains available so funds can be recovered during an emergency.

## 8. Lifecycle

```
CONFIGURE
    │
    ▼
ACTIVE
    │
    ├── EXECUTE ──► ACCOUNT ──► SETTLE
    │
    ├── PAUSE ────► STOP
    │
    ├── REVOKE ───► INACTIVE
    │
    └── EXPIRY ───► EXPIRED
```

Revocation does not erase selector, recipient, or token-policy mappings. If an address is reused, old permissions may become active again; prefer a fresh agent address or explicitly clear and review all permissions before reactivation.

## 9. Application architecture

QPC should remain protocol-agnostic.

A future application can provide:

```
Frontend
   │
   ▼
Agent / API
   │
   ▼
Policy Builder
   │
   ▼
QPCAgentFinance
   │
   ├── DEX adapter
   ├── Lending adapter
   ├── Payment adapter
   └── Other protocol adapters
```

The application chooses **what the agent wants to do**.

QPC decides **whether the agent is allowed to do it**.

## 10. Testing architecture

The test layers should remain separate:

```
Unit / Security
      │
      ▼
Regression tests
      │
      ▼
Invariant tests
      │
      ▼
Stateful handler tests
      │
      ▼
Static analysis
      │
      ▼
External audit
```

A passing test suite is not equivalent to an independent security audit.

## 11. Design principles

1. Least privilege.
2. Human-controlled authority.
3. Deterministic enforcement.
4. Fail closed.
5. Short-lived agent authority.
6. Explicit allowlists.
7. Bounded spending.
8. Emergency stop.
9. No AI custody of owner keys.
10. Keep the core contract small and protocol-agnostic.

## 12. Flitzr-style application flow

The intended user-facing flow can later be presented as:

**Intent → Policy → Preview → Approval → Agent Request → QPC Guardrails → Settlement → Audit**

Unlike an application-specific agent, QPC does not need to know how the strategy was generated. Its job is to enforce the final security policy on-chain.
