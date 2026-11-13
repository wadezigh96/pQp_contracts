# QPC Agent Finance — Architecture

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

## 7. Emergency model

The owner can pause execution globally.

When paused:

- agent execution stops
- token transfers through the agent stop
- deposits through the guarded deposit function stop

Owner withdrawal remains available so funds can be recovered during an emergency.

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


## Exact calldata authorization

Agent contract calls require two independent owner-configured permissions: target plus function selector, and the keccak256 hash of the complete calldata. This prevents an agent from changing encoded arguments (such as recipient, amount, or spender) while reusing a permitted selector. Common ERC-20 allowance-granting selectors are denied in the generic execution path; token transfers should use the dedicated constrained transfer path. This is an incremental defense and does not make arbitrary external protocols safe.


## Privacy, quantum, and interoperability boundaries

The EVM policy contract is not a privacy layer: calldata, storage, events, and transaction metadata are public on transparent chains. Exact calldata hashing binds authorization to arguments but does not hide those arguments.

Post-quantum resistance is a design objective only. This implementation has no PQ signature/KEM, hybrid authentication, or PQ key lifecycle. Likewise, the current repository does not yet contain a production NEAR contract or universal chain adapters. The JavaScript agent-router and Zcash wallet adapter are integration prototypes; Zcash transaction construction, signing, broadcast, and independent shielded-receipt verification remain unimplemented.


## NEAR MVP boundary

The NEAR Rust crate under near/qpc-near-agent-finance is an authorization-and-audit-recording MVP. It is not a deployed contract, escrow, bridge, wallet signer, or settlement verifier. Its policy hash and nonce checks do not make NEAR state private. It has not undergone independent audit or production gas/storage review.


## Cross-L1 authorization and adapter architecture

QPC's target is one versioned authorization envelope enforced independently by each chain adapter, not one universal smart contract. The envelope must bind agent identity, source and destination network/account, action, chain-specific assets, base-unit amount, fee bounds, recipient or privacy commitment, nonce/replay domain, policy ID/version, validity window, and required settlement evidence. The complete proposed field set and verification requirements are specified in [QPC Cross-L1 Authorization Envelope](docs/QPC_CROSS_L1_AUTHORIZATION.md).

The intended adapters are distinct:
- **NEAR:** NEAR account/contract-call and receipt/finality adapter.
- **Ethereum:** EVM chain-ID, transaction, token, receipt, and finality adapter.
- **BSC:** separately configured EVM-compatible adapter with BSC-specific network and finality assumptions.
- **Solana:** program/instruction/account and slot/blockhash/finality adapter.
- **Zcash:** shielded UTXO, wallet/note, nullifier, and shielded-receipt adapter.

A shared envelope does not imply shared transaction semantics. Each adapter must validate its own network identity, asset mapping, replay state, signer boundary, finality, and recovery behavior.

### Current implementation versus target

The NEAR MVP now authorizes the strict canonical v1 envelope using native predecessor authentication and an owner-pinned profile; the legacy limited-hash dispatch is disabled. See [canonical encoding and shared vector](docs/CANONICAL_SPEC.md). Authorization updates persistent replay/cap accounting atomically in NEAR state and emits minimal commitment evidence, not a destination transfer. It does not execute or verify cross-chain settlement.

The local JS intent store persists replay consumption and lifecycle records under a single-host lock and atomic snapshot. It is not a distributed transaction coordinator; uncertain external submissions must enter reconciliation rather than retry automatically. Native preparation handles are unsigned local data, with mock evidence validation explicitly separate from production settlement. See [native adapter boundaries](docs/NATIVE_ADAPTERS.md) and [test evidence](docs/ENGINEERING_EVIDENCE.md).

Ethereum/BSC and Solana production adapters are not implemented. The Zcash wallet adapter is not a production shielded signer/broadcaster or independent receipt verifier. The EVM contract is not private by default. Post-quantum authentication is a design target only and is not implemented.
