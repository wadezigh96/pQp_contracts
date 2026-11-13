# Security Model

## Scope

POA Agent Finance is a policy-control layer for agentic finance. It does not attempt to make an agent trustworthy. Instead, it limits what an agent key can do on-chain.

## Security boundary

The owner controls:

- agent activation and revocation
- native spending caps and per-transaction limits
- token spending policies
- target/function-selector permissions
- token recipient permissions
- emergency pause
- ownership transfer

The agent controls only actions explicitly permitted by the active policy.

## Threat model

### Compromised agent key

Expected result: the attacker can only use permissions that are currently active, subject to expiry, allowlists, per-transaction limits and cumulative caps.

The agent cannot:

- change policy
- change owner
- withdraw native assets as owner
- bypass the target/selector allowlist
- bypass token recipient allowlists
- reset its own spending counters

### Malicious destination contract

A destination contract can attempt reentrancy or return failure data.

Mitigations:

- non-reentrant execution
- accounting before external call
- revert on failed external calls
- explicit target + selector permissions

### Malicious or non-standard ERC-20

Token transfer handling accepts either no return data or an explicit true. Failed transfers revert.

A malformed return payload fails closed rather than being accepted as a successful transfer.

### Compromised owner

The owner is the root authority. A compromised owner can change policies, permissions, pause the system, or withdraw native funds.

This is an intentional trust boundary, not something the agent policy can solve.

Operational protection of the owner key is therefore critical.

### Replay / duplicate execution

Each successful execution consumes a monotonic contract nonce and emits an execution event. The contract itself does not accept off-chain signed intents; replay protection for future signed application layers must be implemented at that layer.

## Policy lifecycle

An active agent policy cannot be overwritten to reset native spending.

An active agent's token policy cannot be overwritten to reset token spending.

The intended lifecycle is:

Configure -> Execute -> Revoke/Expire -> Reconfigure

## Emergency response

The owner can pause the contract. Pause blocks agent execution and deposits through the explicit deposit() function. Native transfers sent directly to receive() remain accepted so funds cannot become permanently stuck solely because the contract is paused.

The owner can revoke an individual agent without affecting other agents.

## Audit status

This repository is not independently audited. Tests are security evidence, not a guarantee of correctness.

Before production deployment:

1. run forge build
2. run forge test
3. run invariant/stateful campaigns
4. run static analysis
5. review deployment configuration
6. obtain independent security review
7. verify the deployed bytecode and constructor owner

## Out of scope

The core contract does not provide:

- AI model safety
- oracle correctness
- protocol-specific economic safety
- private-key custody
- off-chain authentication
- UI security
- cross-chain replay protection
- automatic recovery from a compromised owner
