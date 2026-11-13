# Security Model

## Scope

QPC Agent Finance is a policy-control layer for agentic finance. It does not attempt to make an agent trustworthy. Instead, it limits what an agent key can do on-chain.

## Security boundary

The owner controls:

- agent activation and revocation
- native spending caps and per-transaction limits
- token spending policies
- target/function-selector permissions
- exact calldata-hash permissions
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
- explicit target + selector + exact calldata-hash permissions

### Malicious or non-standard ERC-20

Token transfer handling accepts either no return data or an explicit true. Failed transfers revert.

A malformed return payload fails closed rather than being accepted as a successful transfer.

### Compromised owner

The owner is the root authority. A compromised owner can change policies, permissions, pause the system, or withdraw native funds.

This is an intentional trust boundary, not something the agent policy can solve.

Operational protection of the owner key is therefore critical.

### Replay / duplicate execution

Each successful execution consumes a monotonic contract nonce and emits an execution event. The contract itself does not accept off-chain signed intents; replay protection for future signed application layers must be implemented at that layer.

## Exact call authorization

`execute` requires both a target/selector permission and an owner-approved `keccak256(calldata)` hash. This binds all encoded arguments to the approved call. A changed recipient, amount, spender, or other argument produces a different hash and fails closed. Owners must explicitly approve each exact calldata payload they intend an agent to use.

The `execute` path rejects common ERC-20 allowance-granting selectors (`approve`, `increaseAllowance`, and standard ERC-20/DAI `permit` signatures). This selector denylist is defense-in-depth, not a universal detector for arbitrary token or protocol-specific approval functions. The preferred token path remains `transferToken`, with its recipient allowlist and amount caps.

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


## Privacy and post-quantum claims

The current EVM contract does not make public-chain activity private. Storage, calldata, logs/events, and transaction metadata remain publicly observable according to the underlying chain. A calldata hash is an authorization control, not encryption or a zero-knowledge proof.

No post-quantum signature/KEM, hybrid classical-plus-PQ authentication, PQ identity format, or migration/recovery protocol is implemented here. The project name and roadmap must not be treated as evidence of quantum resistance. Any future PQC integration needs a reviewed scheme, parameter set, implementation, key lifecycle, chain compatibility analysis, and independent cryptographic review.

The current repository also does not establish a production NEAR-to-Zcash bridge or universal L1 interoperability. The Zcash adapter does not yet create, sign, broadcast, or independently verify shielded payments.


## NEAR MVP boundary

The NEAR Rust crate under near/qpc-near-agent-finance is an authorization-and-audit-recording MVP. It is not a deployed contract, escrow, bridge, wallet signer, or settlement verifier. Its policy hash and nonce checks do not make NEAR state private. It has not undergone independent audit or production gas/storage review.


## Cross-L1 authorization threat model

The current EVM contract nonce protects its own execution sequence; it is not a cross-chain replay defense. The NEAR MVP separately checks a per-agent nonce, policy ID/version, fixed action and destination, router, recipient allowlist, amount caps, and a limited payment-argument hash. Neither implementation currently authenticates and enforces the complete cross-L1 authorization envelope.

Before cross-chain activation, every adapter must bind and validate the agent identity, source and destination network/account, action, source/destination asset identifiers and decimals, amount, fee bounds, recipient or privacy commitment, nonce and replay domain, policy ID/version, issue/expiry times, cancellation state, and settlement-evidence requirements. One-field mutation, cross-chain replay, cross-direction replay, stale policy, key revocation, and downgrade tests are mandatory. The proposed protocol fields and acceptance tests are in [QPC Cross-L1 Authorization Envelope](docs/QPC_CROSS_L1_AUTHORIZATION.md).

### Adapter-specific threat boundaries

- **NEAR:** validate predecessor/account identity, contract and method, policy state, nonce, attached deposit, receipts, and finality.
- **Ethereum:** validate chain ID, signer/account nonce, target/value/calldata, token behavior, receipt, and finality.
- **BSC:** validate BSC network identity and its own RPC/finality assumptions in addition to EVM controls.
- **Solana:** validate cluster identity, program IDs, instruction/account metas, signer/PDA authority, blockhash/nonce, and slot commitment.
- **Zcash:** validate network, shielded receiver policy, note/nullifier state, wallet scan trust, and shielded receipt evidence. Never expose spending or viewing keys to agents or logs.

Provider callbacks and transaction identifiers are untrusted inputs until independently checked under the adapter's documented trust model. Reorgs, stale checkpoints, partial execution, timeouts, duplicate delivery, and refund/reconciliation are security states, not mere UI errors.

### Privacy and quantum constraints

Privacy controls must cover identity, authorization, agent communication, and settlement. A hash or opaque recipient reference is not encryption or proof of private execution. Each route must fail closed rather than silently downgrade its required privacy class.

Post-quantum resistance remains a design objective only. Do not enable or advertise PQ security until the selected standardized algorithms, identity binding, key lifecycle, downgrade protections, chain integration, interoperability tests, and independent cryptographic review are complete.
