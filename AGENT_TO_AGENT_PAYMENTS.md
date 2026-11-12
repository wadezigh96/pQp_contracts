# QPC Agent-to-Agent Payments and Interoperability

**Status: architecture proposal only. No agent runtime, cross-chain bridge, relayer, escrow, or payment settlement is implemented by this document.**

## Goal

Enable independently operated agents to discover one another, negotiate a bounded task, authorize payment, deliver verifiable work, and settle payment. Support Ethereum, BNB Smart Chain, Solana, and NEAR without pretending that their contracts, signatures, finality, or token standards are interchangeable.

QPC is the policy and authorization layer. It is not itself a bridge, exchange, custodian, or universal token.

## Components and hard separation

1. **Agent identity and discovery (off-chain):** a signed agent manifest advertises agent ID, supported chains, payment assets, service methods, endpoints, capability claims, and key references. Endpoints and claims are untrusted until independently verified.
2. **Agent-to-agent protocol (off-chain):** versioned request/offer/accept/deliver/settle messages. Messages include unique job ID, expiry, chain ID, contract/program ID, token address or mint, amount, decimals, recipient, quote hash, and replay nonce.
3. **QPC policy adapter (on-chain, chain-native):** validates that an agent may perform a particular action under explicit limits. It does not prove the quality of an AI agent's work.
4. **Payment escrow/settlement (on-chain, separate module):** holds or transfers assets according to a state machine. Keep settlement logic separate from policy logic.
5. **Evidence and dispute process:** records hashes and state transitions. A hash is evidence of bytes, not proof that work is correct. Disputes need an explicitly defined arbitrator or objective verification method.

## Chain-specific deployments

| Network | Native implementation | Payment integration |
|---|---|---|
| Ethereum | Solidity QPC adapter and separate escrow contract | ERC-20 / native ETH using safe token handling |
| BNB Smart Chain | Separate BSC deployment and chain-ID configuration; source may share reviewed EVM core | BEP-20-compatible tokens / native BNB |
| Solana | Independent Rust/Anchor program, PDAs, signer constraints, CPI and token-program validation | SPL Token / Token-2022 only after explicit support and mint validation |
| NEAR | Independent Rust contract and promise/callback state machine | NEP-141 FT and native NEAR only with explicit receiver/storage/deposit handling |

Each chain has its own contract/program address, authority, policy state, token registry, nonce domain, and deployment/release process. No cross-chain state is implied by source-code sharing.

## Same-chain payment flow

1. Agent A discovers Agent B and retrieves B's signed service manifest.
2. A sends a signed quote request containing job scope, maximum price, asset, chain, expiry, and unique job ID.
3. B returns a signed quote. A verifies B's identity/key trust and checks price, asset, chain, expiry, and service terms.
4. A creates or selects a QPC policy that caps target, token, recipient, amount, call count, and expiry.
5. A explicitly approves funding. A chain-local escrow locks the exact amount or uses a clearly disclosed direct-payment flow.
6. B performs the task and returns a result plus content hash and evidence references.
7. The agreed verifier or buyer accepts/rejects before the deadline. Escrow releases funds to B on acceptance or follows a predeclared refund/dispute rule.
8. Both agents store the settlement transaction ID and final job state. Retries are idempotent by job ID and nonce.

Do not release funds merely because an agent says it completed a task. Define the acceptance oracle before funding.

## Cross-chain payment flow

Cross-chain agent messaging and cross-chain asset settlement are separate problems.

- Agents may negotiate across networks using signed off-chain messages.
- The payer funds escrow on the source chain.
- A destination-chain payment must only occur after a verified settlement condition using an explicitly selected bridge, interoperability protocol, or independent relayer/oracle design.
- Never trust a relayer's plain HTTP assertion as proof of source-chain payment.
- Bind every message to source chain ID, source contract, destination chain ID, destination contract, token identifiers, amount, recipient, job ID, expiry, and replay nonce.
- Define finality thresholds, reorg handling, duplicate delivery, timeout, refund, bridge failure, and dispute handling.
- If there is no reviewed bridge/proof mechanism, do not claim atomic cross-chain payment. Use same-chain settlement or stop safely.

A future cross-chain adapter must be separately threat-modeled and audited. Do not implement a homemade bridge.

## Minimum payment state machine

`Created -> Quoted -> Accepted -> Funded -> WorkSubmitted -> AcceptedForPayment -> Settled`

Alternative terminal states: `Cancelled`, `Expired`, `Refunded`, `Disputed`.

Every transition must be authenticated, monotonic, idempotent, and deadline-bound. A dispute must not silently become a successful payment. Refunds must be backed by actual escrow balance and must not permit double withdrawal.

## Required security controls

- Explicit per-agent allowlists and revocation.
- Exact chain ID, contract/program ID, token contract/mint, recipient, amount, and decimals.
- Typed payload schemas; do not rely only on hashing arbitrary JSON bytes.
- Domain-separated signatures binding protocol version, chain, contract, job, nonce, and expiry.
- Replay protection across chains and across protocol versions.
- Per-job and per-agent caps, rate limits, expiry, and emergency pause.
- Reentrancy and safe token-transfer handling on EVM.
- Solana signer/PDA/owner/token-program/mint checks.
- NEAR attached-deposit accounting, storage funding, private callbacks, and asynchronous failure recovery.
- No private keys in contracts, manifests, repository files, CI, or agent prompts.
- AI model output is never treated as authorization.
- Independent review of escrow, bridge, oracle, and token integrations.

## Post-quantum identity boundary

NEAR ML-DSA-65 transaction access-key support, where enabled by the network protocol, authenticates a NEAR transaction at the protocol layer. It does not make QPC a general ML-DSA verifier and does not automatically authenticate agent messages on Ethereum, BSC, or Solana.

Agent message signatures need a chain-specific, independently reviewed trust and verification design. SLH-DSA remains an off-chain experiment until a supported verification path exists. Do not advertise cross-chain PQ authentication as active.

## Build sequence

### Phase 0 — specification and threat model
- Freeze message schemas and state machine.
- Specify supported assets and exact chain IDs.
- Document trust assumptions and failure handling.
- No funds or deployment.

### Phase 1 — local same-chain prototype
- Implement agent simulator A and B.
- Implement signed manifest, quote, accept, and work receipt.
- Build NEAR sandbox escrow proof-of-concept with mock FT and bounded amounts.
- Test replay, wrong recipient, wrong token, amount overflow, expiry, duplicate callbacks, failed promise, and refund invariants.

### Phase 2 — EVM adapters
- Add Ethereum and BSC escrow adapters separately.
- Test ERC-20 return-data edge cases, fee-on-transfer/rebasing token rejection or explicit handling, reentrancy, chain-ID mismatch, and duplicate settlement.
- Independent tests and security review per chain.

### Phase 3 — Solana
- Implement a native Rust/Anchor escrow program.
- Test PDA seeds, signer constraints, token program allowlists, mint/decimal validation, CPI failures, and account lifecycle.

### Phase 4 — cross-chain settlement research
- Compare established interoperability systems and their trust models.
- Select one only after threat model and audit review.
- No cross-chain transfers until source proof, finality, replay, timeout, and refund semantics are tested.

### Phase 5 — PQ identity research
- Benchmark ML-DSA-65 on supported NEAR transaction keys.
- Keep SLH-DSA off-chain until the verification path is real.
- Do not conflate transaction authentication with agent-to-agent message authentication.

## Demo acceptance criteria

A truthful first demo should show two local/test agents on one network:
- discover and verify each other's manifests;
- negotiate a bounded quote;
- require payer approval;
- fund a mock or testnet escrow;
- submit a result;
- release or refund exactly once;
- show replay and unauthorized-recipient rejection;
- display transaction and event evidence.

Cross-chain demo claims require actual end-to-end settlement on both chains and verified interoperability evidence. A diagram or simulated relayer is not a cross-chain payment.

## Current implementation status

The current repository has a NEAR policy/dispatch prototype and separate EVM work. This document does not add an agent runtime, escrow, token transfer, bridge, Solana program, or active PQ signature verifier. Those are future implementation milestones. No testnet/mainnet deployment or real payment is authorized by this proposal.
