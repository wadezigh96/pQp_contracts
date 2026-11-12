# PQP Security Model

## Scope

PQP is an on-chain policy and agent-protection framework with a post-quantum migration strategy. This document distinguishes implemented controls from planned controls; it is not a claim that the system is immune to compromise.

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

## Implemented NEAR policy-core controls

The first NEAR vertical slice in `near/pqp-near` enforces:
- owner-only policy creation, versioned updates, pause and irreversible revocation;
- immutable agent identity;
- bounded policy validity;
- exact target, method and argument-digest matching;
- per-call, per-action cumulative and policy-wide deposit caps;
- sequential nonces and unique request IDs;
- fail-closed rejection for unauthorized callers, stale versions, paused/revoked/expired policies and over-budget requests.

The current contract records authorization and consumes budget only. It does not call target contracts or transfer assets. The execution adapter, callback state machine, guardian controls, and post-quantum transaction integration are not implemented yet.

## Threats and required controls

### Compromised agent key
Short expiry, per-action and cumulative limits, explicit target/method/argument constraints, destination restrictions, nonce/replay domain, revoke and pause. Agent never receives owner privileges.

### Malicious destination or argument substitution
Avoid unrestricted generic call forwarding. Validate security-critical calldata arguments, not only selectors. The current NEAR policy core compares an exact 32-byte argument digest; the future adapter must define and test canonical argument encoding and compute the digest itself before dispatch. Bound attached value and token approvals. Restrict approval, role-change, admin and upgrade methods by default.

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

## Legacy EVM prototype — audit findings

The Solidity prototype is retained as an audit input, not production PQP code.

1. HIGH — generic call arguments are not semantically validated; selector permission alone cannot prevent malicious argument substitution.
2. HIGH — generic forwarding is too broad for approvals, role changes, admin actions, arbitrary transfers and upgrades without typed adapters and explicit denials.
3. MEDIUM — prior Foundry invariant configuration used an unsupported target setting; CI configuration must be validated against the installed Foundry version.
4. BUILD BLOCKER — the stateful test cast must convert the address to a payable contract type before compilation.
5. GAP — stateful/invariant coverage must exercise malicious arguments, pause/revoke, replay and failed external calls.
6. TRUST — owner authority remains a root trust boundary; multisig/timelock and rehearsed recovery are required.
7. GAP — no independent security audit is documented.
8. GAP — the EVM prototype contains no post-quantum cryptographic implementation.

## Release gates

- No production funds before external audit and remediation.
- No arbitrary-call executor without typed method schemas and argument validation.
- No guardian transfer authority.
- No cryptographic claim without verified runtime, wallet and transaction support.
- All policy state transitions and adversarial cases must have tests.
- CI success is not an independent audit or a guarantee of security.
