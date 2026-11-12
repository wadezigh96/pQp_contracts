# PQP Security Model

## Scope

PQP is a proposed on-chain policy and agent-protection framework with a post-quantum migration strategy. This document describes intended controls, not a claim that they are already implemented.

## Trust boundaries

| Component | Trust assumption |
|---|---|
| Owner/governance | Root authority; must use multisig, hardware-backed keys and recovery |
| Agent | Potentially compromised; least privilege only |
| PQP contract/program | Deterministic enforcement, subject to bugs and host-chain behavior |
| L1 consensus | External dependency; PQP cannot repair consensus vulnerabilities |
| RPC/indexer | Untrusted for authorization unless independently verified |
| Monitor/AI detector | Advisory and fallible; no unrestricted custody |
| Bridge/relayer/MPC | Independent trust boundary |
| External protocol | May be malicious, compromised or economically unsafe |

## Threats and required controls

### Compromised agent key
Short expiry, per-action and cumulative limits, explicit target/method/argument constraints, destination restrictions, nonce/replay domain, revoke and pause. Agent never receives owner privileges.

### Malicious destination
Avoid unrestricted generic call forwarding. Validate security-critical calldata arguments, not only selectors. Bound attached value and token approvals. Restrict approval, role-change, admin and upgrade methods by default.

### Replay and duplicate requests
Bind chain/network domain, contract ID, policy ID/version, agent identity and nonce into signed requests. Nonce consumption must be atomic with authorization. Cross-chain messages need source/destination domains, unique message IDs and finality validation.

### Reentrancy and asynchronous callbacks
EVM adapters require reentrancy protection and checks-effects-interactions. NEAR adapters must model asynchronous receipts, callback authentication, duplicate callbacks and partial failures. Solana adapters must validate account ownership, signer status, PDA seeds and writable account constraints.

### Owner/governance compromise
Use multisig or threshold governance, timelock for high-impact changes, separate emergency pause from asset withdrawal, and rehearse recovery. Do not claim protection against a fully compromised root authority.

### Quantum-capable adversary
Inventory all signature and key-agreement dependencies. Prefer standardized PQ signatures where the chain's native transaction layer supports them. Provide versioned crypto-agility and key-rotation metadata. Do not confuse ML-KEM key establishment with digital signatures. Verify the full path: wallet, account key, transaction validation, validators/consensus, bridge and dependent services.

### Monitor or AI compromise
Monitoring is not a source of truth. Alerts may trigger only explicitly granted narrow pause powers. No monitor-controlled transfer, withdrawal or policy expansion.

## Inherited POA prototype — audit findings

The legacy Solidity POAAgentFinance contract is retained as an audit input, not PQP production code.

1. HIGH — generic call arguments are not validated. execute() checks only target and 4-byte selector. An allowed selector can still carry attacker-controlled recipient, spender, amount or other dangerous arguments.
2. HIGH — generic call forwarding is too broad for a security product. There is no per-method argument schema or typed action adapter. Sensitive methods such as token approvals, role changes, arbitrary transfers and upgrade calls need explicit denial or dedicated validation.
3. MEDIUM — invariant configuration contains an unsupported key. foundry.toml uses target_contracts; Foundry CI reports it as unknown, so intended invariant targeting is not configured.
4. BUILD BLOCKER — latest observed stateful test cast fails. GitHub Actions fails at test/POAAgentFinance.stateful.t.sol:14 because a non-payable address is cast to a contract with a payable receive function. The test step is skipped after build failure.
5. TEST GAP — handler scaffolding is not proof of meaningful invariant campaigns. The handler has a construction test; the invariant harness does not cover all action paths or malicious argument combinations.
6. ROOT TRUST — owner can change policies and withdraw native funds. This must be protected with multisig/timelock and documented.

These findings are based on source and the latest observed GitHub Actions run for main commit 76e23e1a50a6d23f188c6954c07bb78d8c28fa9e (26 September 2026). They have not been independently audited.

## Security acceptance criteria

- No arbitrary call forwarding without typed argument validation.
- No policy mutation by agent identities.
- No spending counter reset through policy edits.
- Expiry, revocation and pause fail closed.
- Signatures bind to domain, policy version and nonce.
- Async/cross-chain states are explicit and idempotent.
- Upgrade and recovery authority are threshold-controlled.
- Adversarial tests cover malformed inputs, callbacks, replays, malicious targets and compromised agents.
- Independent audit before production funds.

## Audit status

PQP is not independently audited. The inherited POA prototype has known issues listed above. No claim of production readiness, hack-proof security or end-to-end post-quantum security is made.
