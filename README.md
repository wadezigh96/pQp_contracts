# QPC --- QUANTUM PERSISTENCE CONTRACT

<div align="center">

**Policy-Controlled Agentic Finance Infrastructure**

**Human intent → Policy → Agent execution → On-chain settlement → Audit**

</div>

---

## About

QPC Agent Finance is a standalone smart-contract foundation for autonomous and AI-assisted finance.

The core principle is simple:

> **The agent may execute within a policy, but the agent never becomes the owner.**

The contract separates **human authority** from **agent execution authority**. Policies define what an agent can do, how much it can spend, where it can send funds, which contract functions it can call, and when its authority expires.



## Core workflow

```mermaid
flowchart TB
    U[Human / Owner] --> P[Create Agent Policy]
    P --> G[Guardrails]

    G --> A[Agent Authority]
    G --> C[Call Allowlist]
    G --> R[Recipient Allowlist]
    G --> T[Token Policy]
    G --> E[Expiry]

    A --> X[Agent Request]

    X --> V{Policy Check}
    V -->|blocked| B[Reject]
    V -->|allowed| S[State Accounting]

    S --> Q[On-chain Execution]
    Q --> L[Nonce + Events]
    L --> Z[Settlement]

    U -->|Pause / Revoke| G
```

### Execution path

**Configure → Activate → Request → Validate → Account → Execute → Emit event**

1. Owner configures an agent.
2. Agent receives limited authority.
3. Agent submits an execution request.
4. Contract checks expiry, pause state, target, selector, exact calldata hash, recipient and spending limits.
5. Spending accounting is updated.
6. The approved call or token transfer executes.
7. Nonce and event data provide an on-chain execution trail.
8. Owner can pause or revoke authority at any time.

---

## Security architecture

```mermaid
flowchart LR
    O[Owner] -->|policy control| PC[QPC Policy Controller]

    PC --> AP[Agent Policy]
    PC --> CL[Call Allowlist]
    PC --> RL[Recipient Allowlist]
    PC --> TP[Token Policy]
    PC --> EX[Expiry]
    PC --> PP[Emergency Pause]

    AG[Agent Key] -->|limited request| G{Guardrails}
    AP --> G
    CL --> G
    RL --> G
    TP --> G
    EX --> G
    PP --> G

    G -->|pass| CE[Contract Execution]
    G -->|fail| RE[Revert]

    CE --> EV[Nonce + Events]
    CE --> SET[Settlement]
```

### Security boundaries

| Layer | Responsibility |
|---|---|
| Owner | Creates, changes, pauses and revokes policies |
| Agent | Requests permitted execution only |
| Policy | Defines limits and permissions |
| Contract | Enforces policy deterministically |
| Nonce | Provides monotonic execution sequencing |
| Events | Provide an on-chain audit trail |
| Pause | Emergency global execution stop |
| Expiry | Automatic authority termination |
| Reentrancy guard | Prevents nested execution |

AI logic is **not trusted as a security boundary**. The smart contract remains the final enforcement layer.

---

## Project structure

```
qpc_contracts/
├── contracts/
│   └── QPCAgentFinance.sol       # Core policy + execution contract
│
├── test/
│   ├── QPCAgentFinance.security.t.sol
│   │                              # Security regression tests
│   ├── QPCAgentFinance.invariant.t.sol
│   │                              # State/invariant checks
│   └── QPCAgentFinance.stateful.t.sol
│                                  # Stateful handler scenarios
│
├── near/\n│   └── qpc-near-agent-finance/    # NEAR Rust policy contract MVP\n├── agent-router/                  # Local-only cross-chain request preparation\n├── zcash/                         # Shielded wallet adapter boundary\n├── script/                        # Deployment scripts
├── foundry.toml                   # Foundry compiler/test configuration
├── README.md                      # Architecture + usage overview
└── ARCHITECTURE.md                # Detailed system design
```

The repository keeps the **security-critical policy layer small and isolated**. Application-specific agents, APIs, wallets and protocol adapters can be built above it later without changing the core security model.

---

## Policy model

### Native assets

Each agent receives:

- maximum policy duration: **30 days**
- total native spending cap
- per-transaction native limit
- active / revoked state

### ERC-20 assets

Each agent/token pair can define:

- cumulative token cap
- per-transaction token limit
- cumulative spending accounting

### Contract calls

Calls require an explicit:

**agent + target + function selector + exact calldata hash**

allowlist entry.

### Token recipients

Token transfers require an explicit:

**agent + recipient**

allowlist entry.

---

## Owner lifecycle

```
Create Policy
     ↓
Activate Agent
     ↓
Agent Executes
     ↓
 ┌───────────────┐
 │ Active Policy │
 └───────────────┘
     ↓
  Pause / Revoke / Expire
     ↓
Authority Ends
```

The owner remains the root authority throughout the lifecycle.

---

## Agent execution lifecycle

```
Agent
  │
  ▼
Request
  │
  ▼
Is active?
  │ no ─────► Revert
  │ yes
  ▼
Is paused?
  │ yes ────► Revert
  │ no
  ▼
Policy validation
  │
  ├── expiry
  ├── target + selector
  ├── recipient
  ├── per-tx limit
  └── cumulative cap
  │
  ▼
Accounting
  │
  ▼
On-chain call
  │
  ▼
Nonce + event
  │
  ▼
Settlement
```

---

## Security status

Current defenses include:

- two-step ownership transfer
- agent expiry
- native spending caps
- native per-transaction limits
- ERC-20 spending caps
- ERC-20 per-transaction limits
- target/function-selector allowlisting
- exact calldata-hash allowlisting for every agent call
- rejection of common ERC-20 allowance-granting selectors (`approve`, `increaseAllowance`, `permit`)
- recipient allowlisting
- emergency pause
- monotonic nonce
- reentrancy protection
- defensive ERC-20 transfer handling
- policy validation
- security regression tests
- invariant/stateful test scaffolding

This repository has **not been independently audited**. Passing local or CI tests does not constitute a security audit. Production funds should not be deployed until independent review, fuzzing, static analysis and deployment validation are complete.

---

## Development

```bash
forge build
forge test
```

The intended development loop is:

**Change → Build → Security tests → Invariant/stateful tests → Review → Deploy**

---

## Future application layer

The core contract intentionally does not contain protocol-specific logic.

A future application can sit above it:

```
User / AI Agent
      │
      ▼
Application / API
      │
      ▼
Policy Builder
      │
      ▼
QPCAgentFinance
      │
      ├── DEX
      ├── Lending
      ├── Payments
      └── Other protocols
```

This keeps integrations replaceable while the security boundary remains centralized in the QPC contract.

---

<div align="center">

**Policy first. Agent second. Settlement last.**

</div>


## Privacy and quantum-resistance status

The name **Quantum Persistence Contract** describes the project's direction, not a currently achieved cryptographic guarantee.

- The current EVM contract is public-chain software. Contract storage, calldata, transaction metadata, and emitted events are observable; the contract does not provide private on-chain execution by itself.
- No post-quantum signature, key-encapsulation mechanism, hybrid classical/PQ authentication, or PQ key-rotation/recovery protocol is implemented in this repository's current contract.
- The current cross-chain work is a prototype boundary, not a universal L1 network. The repository does not yet contain a production NEAR smart contract or production adapters for all L1s.
- The Zcash adapter currently provides address/policy validation, local wallet database initialization, replay protection, and a read-only lightwalletd probe. It does not yet scan shielded notes, construct/sign/broadcast payments, or independently prove shielded receipts.

Do not describe QPC as quantum-resistant, private-on-chain, a live bridge, or production-ready until those capabilities are implemented and independently reviewed.


## NEAR contract status

A NEAR Rust policy-contract MVP now lives in near/qpc-near-agent-finance. It checks agent authorization, router binding, policy version, nonce, expiry, payment-argument hash, and spending caps before recording an authorization event. It does not execute a Zcash payment, call a router, or prove settlement. It has not been deployed to NEAR Testnet or independently audited. See that crate's README and the NEAR contract CI workflow.
