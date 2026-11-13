# QPC — NEAR-First Hackathon Technical Specification

**Project:** QPC — Quantum Persistence Contract  
**Positioning:** Private on-chain infrastructure for policy-controlled agentic finance  
**Hackathon primary track:** Open Innovation (`open`)  
**Additional fitting tracks:** Privacy & Private AI (`privacy`), AI Security & Defense (`security`), Agent Economy (`agents`)  
**Status:** NEAR policy-gate source prototype added; build/CI verification pending. Private settlement, x402 execution, TEE attestation integration, and post-quantum verification are not implemented.

---

## 1. Executive summary

QPC proposes a NEAR-first, chain-extensible policy and payment-control layer for autonomous agents. A human or organization defines an authorization policy; an agent can request a payment or action only within that policy; a chain-specific adapter validates and executes the approved operation; and the system produces a verifiable audit record without placing the owner's root authority in the agent.

The new idea for **Open Innovation** is to combine three concerns that are usually treated separately:

1. **Agent authority:** explicit, bounded, revocable permissions.
2. **Privacy-aware payment routing:** a payment request must declare its privacy requirements, and the router must reject unsupported privacy modes rather than silently downgrade to a transparent transfer.
3. **Chain abstraction:** a common policy intent can be evaluated consistently while settlement remains specific to each chain and payment rail.

QPC is not claiming to make every L1 private, quantum-resistant, or immune to exploits. It defines interfaces and verification requirements so those properties can be implemented and assessed per adapter.

## 2. Problem statement

Agents increasingly need to pay APIs, other agents, compute providers, and services. Giving an agent an unrestricted wallet key creates excessive authority. A normal on-chain transfer may expose payment metadata. A payment negotiation protocol alone does not solve privacy, authorization, replay, or key-management risks. Cross-chain execution further complicates asset identity, recipient validation, finality, and failure recovery.

QPC addresses this as a policy-enforcement and payment-routing problem rather than trusting an AI model to behave safely.

## 3. Design principles

- **Human remains root authority:** an agent cannot grant itself permissions, change policy ceilings, or replace the owner.
- **Least privilege:** every agent receives only the permissions needed for a task.
- **Fail closed:** invalid, expired, replayed, or unsupported requests are rejected.
- **Privacy must be explicit:** every payment declares its privacy mode and acceptable disclosure.
- **No silent privacy downgrade:** if the requested shielded/private rail is unavailable, the payment is rejected or held for explicit human action; it is never transparently rerouted without consent.
- **Chain-specific execution:** a shared interface does not imply identical contract semantics across L1s.
- **Cryptographic agility:** algorithms and key formats must be replaceable through reviewed versioned interfaces.
- **Evidence before claims:** a capability is marked implemented only after code, tests, and demo evidence exist.

## 4. System architecture

### 4.1 Logical components

| Component | Responsibility | Hackathon status |
|---|---|---|
| Policy registry / controller | Owner-defined agent permissions, limits, expiry, revocation | Prototype target |
| Agent request gateway | Receives normalized payment/action intents | Prototype target |
| Policy evaluator | Validates identity, scope, amount, recipient, expiry, nonce and privacy mode | Prototype target |
| NEAR adapter | Executes or records approved NEAR-side operations | Prototype target |
| x402 adapter | Parses HTTP 402 payment requirements and binds them to a QPC policy decision | Interface/design; end-to-end integration pending |
| Privacy policy router | Enforces declared privacy requirements and refuses unsupported rails | Prototype target; privacy guarantees depend on selected rail |
| Audit/event layer | Emits request ID, policy version, outcome and public-safe evidence | Prototype target |
| Other L1 adapters | Chain-specific policy and settlement implementations | Future |
| Post-quantum crypto module | Standardized PQ verification and migration | Research/future; not active |

### 4.2 Request lifecycle

```
Human defines policy
        |
        v
Agent creates payment intent
        |
        v
Normalize request (chain, asset, amount, recipient,
expiry, nonce, service, privacy requirement)
        |
        v
QPC policy evaluation
        |
        +---- invalid / expired / over limit ----> REJECT
        |
        +---- privacy rail unsupported ----------> REJECT / HOLD
        |
        v
Human approval if policy requires it
        |
        v
Chain + payment adapter
        |
        v
Settlement / execution result
        |
        v
Public-safe audit event + private receipt reference
```

The policy engine must not treat model output as proof of authorization. Authorization must be validated from cryptographic credentials and contract state.

## 5. Policy model

A policy should include, at minimum:

- owner / policy controller identity
- agent public key or scoped credential identifier
- allowed action types and service identifiers
- allowed chain and asset identifiers
- maximum amount per payment
- cumulative spending ceiling and accounting window
- recipient or merchant constraints
- policy activation and expiry timestamps
- nonce / replay-protection domain
- privacy mode: `transparent`, `confidential-computation`, or `shielded-settlement`
- minimum privacy requirement and allowed fallback behavior
- optional human approval threshold
- policy version and revocation status

The exact storage layout and serialization must be adapted to NEAR's account and contract model. Do not copy Solidity storage assumptions into a NEAR contract.

## 6. Privacy model and honest disclosure

### 6.1 Privacy classes

**Transparent settlement:** public chain data may expose sender/account, recipient, amount, asset, timing, and contract interaction. It must never be labelled private.

**Confidential computation:** sensitive policy evaluation or agent context may be processed in a confidential environment such as a TEE, subject to attestation, implementation, side-channel, operator, and availability risks. A TEE does not automatically hide public settlement metadata.

**Shielded settlement:** sender/recipient/value confidentiality must be provided by a payment rail designed for shielded transactions. The adapter must verify that the selected rail actually supports the requested properties.

### 6.2 Fail-closed routing

If a request specifies `shielded-settlement`, QPC must not send it through a transparent NEAR transfer simply because the private rail is unavailable. The request must fail or remain pending for an explicit human decision.

The router must disclose residual metadata, including possible timing correlation, service endpoint metadata, network-level information, and public policy or commitment data.

### 6.3 NEAR-first implementation boundary

NEAR is the first target for the hackathon prototype, but ordinary NEAR contract state and transfers must be treated as public unless a specific, verified privacy mechanism is used. NEAR AI Cloud / TEE can be evaluated for confidential computation; it must not be represented as a native shielded-payment feature.

A separate shielded payment rail may be integrated later through an adapter. Each rail needs its own threat model, key custody model, recovery procedure, and proof of settlement.

## 7. x402 payment integration

x402 is treated as a payment negotiation/interface layer, not as a privacy system.

Planned flow:

1. A service responds with an HTTP 402 payment requirement.
2. The x402 adapter parses network, asset, amount, recipient, and expiry.
3. QPC binds those fields to a unique request ID and the agent's policy.
4. Policy evaluation checks spending ceilings, recipient/service permissions, expiry, nonce, chain and privacy requirement.
5. If permitted, the selected chain/payment adapter prepares the payment.
6. Settlement evidence is returned to the service using the supported x402 mechanism.
7. QPC records a public-safe result and a reference to any private receipt.

Required protections:

- reject mismatched network, token, recipient, amount, or expiry
- prevent replay through request IDs/nonces and domain separation
- bind approval to the exact payment payload
- do not trust a client-supplied "paid" boolean
- verify settlement through the selected rail's authoritative evidence
- do not silently replace a requested shielded rail with transparent settlement
- keep credentials and secret payment metadata out of public events and logs

The precise x402 version, facilitator, settlement network, and SDK must be selected and tested before claiming end-to-end compatibility.

## 7A. NEAR AI Cloud integration — confidential agent inference

NEAR AI Cloud is a potential confidential-computing component for QPC's agent reasoning and policy-assistance layer. It is separate from NEAR smart-contract execution and separate from payment settlement.

### Appropriate role in QPC

- Run an agent's sensitive reasoning or private policy-assistance workload using a model explicitly identified as **Private TEE**.
- Where supported, use the NEAR AI inference SDK to verify fresh gateway/model attestation evidence and response signatures before treating the inference result as coming from the expected confidential environment.
- Bind verification to a fresh nonce and the expected model/deployment identity; reject stale, malformed, mismatched, or unverifiable evidence.
- Keep the deterministic QPC policy evaluator authoritative. An LLM may propose an action or explain a policy decision, but its text output is never authorization and never overrides on-chain policy.
- Minimize prompt contents. Do not send signing keys, seed phrases, raw wallet secrets, or data unnecessary for the inference task.

### Integration boundary

```
Agent / QPC application
       |
       +--> NEAR AI Cloud API (confidential inference) [OPTIONAL COMPONENT]
       |       |
       |       +--> Private TEE model
       |       +--> Fresh attestation + signed response verification
       |
       v
Deterministic QPC policy evaluator
       |
       v
NEAR contract / chain adapter
       |
       v
Payment rail and settlement
```

The macOS NEAR AI Gateway desktop application is a developer workflow tool for launching supported coding agents against NEAR AI Cloud and reviewing local traces. It is not a NEAR smart contract, payment rail, or requirement for a deployed QPC application. A QPC backend should integrate with the Cloud API/SDK directly where appropriate.

### Privacy tiers and limitations

- **Private TEE models:** model inference runs in a hardware-backed confidential environment and can provide attestation evidence. Verify the exact model and gateway evidence; do not infer this property from the provider name alone.
- **Incognito / external-provider models:** account routing may be protected, but the external model provider processes the request. This is not equivalent to private TEE inference and must not satisfy a strict Private-TEE-only policy.
- Attestation proves specific hardware/software measurements and signed evidence under stated trust assumptions. It does not prove the model is correct, the code is bug-free, or the overall agent is safe.
- Gateway infrastructure may retain usage metadata. Temporary caches and timing behavior may also have privacy implications. Review the current retention, cache, and deployment manifests before sending sensitive production data.
- TEE inference protects data during computation within the attested boundary; it does **not** make NEAR transactions, account balances, recipients, or payment amounts private.
- It does not provide shielded settlement, x402 payment execution, cross-chain privacy, or post-quantum cryptography.

### Hackathon acceptance criteria for this component

1. Use a specifically listed Private TEE model, not an external-provider/Incognito model.
2. Verify a fresh nonce-bound gateway and model attestation using the official verifier/SDK path.
3. Verify the response signature and bind the verified result to the request/model identity.
4. Demonstrate rejection of stale nonce, wrong model, invalid signature, and unavailable verification.
5. Show that QPC's deterministic policy checks still reject over-limit, wrong-recipient, expired, and replayed payment intents regardless of what the model recommends.
6. Document which prompt data is sent, what metadata remains visible, and what evidence is retained.

This integration is optional for the first vertical slice. Do not make it a hard dependency for contract execution or payment settlement.

## 8A. NEAR policy-gate source prototype — implemented, verification pending

The repository now includes a first NEAR-native Rust contract prototype at `near/qpc-policy/`.

### Implemented in source

- Owner-only agent policy configuration.
- Maximum 30-day policy duration.
- Per-payment maximum and cumulative authorized cap.
- Recipient allowlist capped at 64 unique account IDs.
- Agent revocation and global pause.
- Sequential per-agent nonce and request-hash format validation.
- Policy versioning and public-safe event logs.
- Unit-test cases for allowed intent, wrong recipient, per-payment limit, cumulative cap, replay, expiry, pause, revocation, unauthorized owner action, duplicate recipients, and policy reconfiguration.

### Exact limitation

`authorize_payment` is an **authorization/reservation operation only**. It does not transfer NEAR, NEP-141 tokens, or any other asset; it does not call a merchant contract; it does not verify an x402 facilitator; and it does not prove settlement. Authorized amounts are conservatively accumulated against the cap. There is not yet a cancellation/release or settlement-finalization lifecycle, so this source prototype must not be used to control real funds.

The contract state, call arguments, account identifiers, and transaction metadata are public on NEAR. It is not a private/shielded contract. The request hash is a binding/audit field, not proof of payload correctness.

### Verification status

The source includes unit tests and a GitHub Actions workflow for `cargo test` and a `wasm32-unknown-unknown` check. The tests and WASM compilation must pass on the branch before this prototype is considered validated. No testnet deployment or real-value transaction has been performed.

## 8. NEAR-first prototype scope

The hackathon prototype should demonstrate one narrow, working vertical slice rather than claim a universal system.

### Required demo slice

1. Create a policy for a test agent with a fixed recipient/service, asset, amount ceiling, expiry and nonce domain.
2. Submit a valid agent payment intent.
3. Show QPC accepting the request only when every policy check passes.
4. Submit an over-limit, wrong-recipient, expired, and replayed request; show each rejected.
5. Submit a request requiring shielded settlement when no verified shielded rail is configured; show a fail-closed rejection/hold, not a transparent transfer.
6. Demonstrate a testnet NEAR transaction only for the explicitly supported transparent/test settlement path, if implemented.
7. Show a public-safe audit record with request ID, policy version, decision and settlement reference.

### Explicitly out of scope for the initial demo

- claiming that NEAR transfers are shielded
- production custody or real-value payments
- universal support for all L1s
- production x402 compatibility without a real tested adapter
- Zcash or another shielded rail without a verified adapter and recovery model
- post-quantum signature verification
- claims of immunity to hacking, bugs, or quantum attacks

## 8C. Cybersecurity controls in the NEAR policy gate

The source prototype now includes a defense-in-depth baseline: owner-only administration, two-step owner rotation, a pause-only emergency guardian, bounded policy duration and recipient list, a lifetime cap on unique policy records, sequential nonces, checked arithmetic, and an on-chain SHA-256 digest derived from the deployment domain, contract account, agent, policy version, nonce, recipient, and amount.

The current digest does not bind an asset, service, network, or settlement expiry. The contract now records reservation, settlement, and release states, but does not transfer assets or independently verify settlement. The configured adapter remains a trusted attester. The owner account remains a critical trust root; production ownership should be assigned to a properly configured NEAR multisig.

See [QPC NEAR Policy Gate Security Model](QPC_NEAR_POLICY_SECURITY.md) for threats, residual risks, and release gates. These controls are not a security certification; compile, tests, fuzzing, and independent review remain required.

## 9. Security and adversarial requirements

Threat model and tests must cover:

- unauthorized agent and forged credential
- owner/policy takeover and privilege escalation
- replay, duplicate request, stale nonce, and cross-chain replay
- amount, asset, network, recipient, or expiry substitution
- integer overflow/underflow and decimal normalization errors
- reentrancy and malicious external contracts where applicable
- asynchronous execution, timeout, partial failure and duplicate callbacks
- compromised agent key and policy revocation races
- TEE attestation failure, stale measurements, compromised host assumptions, and side-channel limitations
- privacy downgrade, metadata leakage, and receipt correlation
- adapter misconfiguration and unsupported chain behavior
- dependency, compiler, build, and deployment supply-chain risks

Required controls include least privilege, explicit domain separation, bounded amounts, expiry, replay protection, deterministic policy checks, safe external-call handling, revocation, pause/recovery procedures, versioned adapters, fuzzing, invariant tests, and independent review.

A test suite reduces uncertainty; it does not prove the absence of vulnerabilities. No "unhackable" or "bug-proof" claim is permitted.

## 10. Multi-L1 adapter contract

Each future L1 adapter must publish a capability manifest:

- chain ID / network identifier and environment
- execution and finality model
- supported asset identifiers and decimal rules
- account/key model
- policy enforcement location
- privacy modes genuinely supported
- cryptographic algorithms and signature verification method
- settlement proof/evidence format
- timeout, cancellation, and recovery behavior
- known metadata exposure and limitations
- test coverage and independent audit status

An adapter must declare unsupported features rather than emulate them insecurely. A NEAR implementation, EVM implementation, and UTXO/shielded adapter are separate implementations behind a common intent interface.

## 11. Quantum-resilience roadmap

"Quantum Persistence" is the project name and long-term objective, not a claim that the current prototype is quantum-safe.

Before any post-quantum claim, QPC must:

1. Identify every signature, key agreement, hash, and commitment used.
2. Select applicable standardized algorithms (for example, ML-DSA for signatures and SLH-DSA as a hash-based signature option) based on the chain's verification support and security review.
3. Define key formats, signature encoding, gas/size limits, rotation and recovery.
4. Consider hybrid classical + post-quantum migration where technically sound.
5. Test verification vectors, malformed inputs, downgrade resistance, and interoperability.
6. Obtain independent cryptographic review.

The current Solidity contract does not implement ML-DSA, SLH-DSA, or post-quantum verification. The NEAR prototype must not claim otherwise unless implemented and measured.

## 12. Hackathon track strategy

**Primary: Open Innovation (`open`).** QPC's central contribution is a new composition of human-bounded agent authority, privacy-policy-aware payment routing, and chain-specific settlement with fail-closed privacy behavior.

**Additional tracks to select if the final prototype genuinely demonstrates them:**

- **Privacy & Private AI (`privacy`):** only if the demo implements a real confidential-computation or privacy-preserving payment component and clearly states what is and is not hidden.
- **AI Security & Defense (`security`):** if the demo proves policy enforcement, spending limits, scoped keys/credentials, revocation, and adversarial rejection cases.
- **Agent Economy (`agents`):** if an agent actually requests and completes a service/payment flow.

Do not select tracks based only on roadmap promises. The submission form allows multiple fitting tracks, but each selected track should be supported by the working demo.

## 13. Acceptance criteria for a credible submission

- A public repository with reproducible setup instructions.
- A runnable prototype, not only architecture diagrams.
- A short demo showing one successful allowed flow and several rejected adversarial flows.
- Test output tied to the submitted commit.
- README clearly labels implemented, partial, simulated, and planned components.
- No claim of private NEAR settlement unless a real private rail is used.
- No claim of post-quantum security unless the algorithms are implemented and verified.
- A public demo post and repository link submitted before the event deadline.

## 14. Current baseline disclosure

The existing `QPCAgentFinance.sol` is an EVM/Solidity prototype with owner-managed agent policies, spending limits, allowlists, expiry, pause, nonce/events, and reentrancy protection as documented in the repository. It is not a NEAR contract and does not currently provide private settlement, x402, multi-L1 execution, or post-quantum cryptography.

The NEAR-first prototype described in this document is a proposed new implementation slice. Do not treat this specification as evidence that it has been built.
