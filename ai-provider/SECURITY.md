# QPC AI Provider — Security Boundary

**Status:** security requirements plus implemented prototype controls. Mock tests cover selected boundaries; live cryptographic and deployment verification still require an independent review.

## Assets to protect

- Chain and wallet signing authority.
- Seed phrases, private keys, spending keys, viewing keys, and wallet databases.
- Private recipient data, including Zcash Unified Addresses and payment metadata.
- QPC policy configuration, allowlists, spending limits, and replay state.
- Provider credentials, prompts, model outputs, and attestation evidence.

## Trust boundaries

1. **Agent Runtime:** owns policy evaluation and decides whether an AI result may be used.
2. **AI Provider Interface:** validates provider/model selection and required verification; it has no wallet signer.
3. **Provider / inference gateway:** external, potentially unavailable or compromised; its output is untrusted.
4. **Attestation/signature verifier:** validates evidence using pinned, reviewed verification code and trusted roots.
5. **QPC Policy Gate + chain/wallet adapters:** sole authorization path for payment actions.

The provider interface must not be placed inside the wallet signing boundary and must not receive signer callbacks.

## Required invariants

- No provider response can grant permissions, change policy, add an allowlist entry, raise a spending cap, disable a pause, or authorize a transfer.
- No wallet signer, private key, mnemonic, Zcash spending key, or unrestricted transaction function is reachable from the AI adapter.
- A policy requiring TEE evidence rejects gateway-only signatures.
- Attestation is bound to the requested model, deployment identity, signer, freshness window, and expected trust root.
- A response signature is verified over exact canonical request/response bytes; parsing and re-serialization must not silently change the signed payload.
- Missing, expired, malformed, mismatched, or unverifiable evidence fails closed.
- No silent provider/model/privacy downgrade. Fallback is disabled unless separately and explicitly configured.
- Timeouts and provider errors do not create a payment, sign a transaction, or mark a payment settled.
- Secrets and private payment metadata are redacted from logs, errors, telemetry, and test output.
- AI-generated instructions are treated as untrusted content, including instructions that ask the runtime to reveal secrets or bypass policy.

## NEAR AI evidence distinction

The integration must preserve the difference between a gateway signature and a provider-TEE signature. A gateway signature proves only the gateway's signed statement under its verification model; it is not proof that the model executed inside a provider TEE. Where policy requires model TEE execution, require the provider-TEE evidence and separately validated deployment attestation.

The adapter pins `@nearai/inference-sdk@0.3.0`, enables E2EE, invokes the SDK's model verification before every inference, checks the SDK-reported deployment provenance and a local compose-hash allowlist, then verifies the response receipt. It accepts only `provider_tee` receipts. Do not infer that mock tests validate the SDK cryptography or trust roots; those still require independent review.

## Threat cases

| Threat | Required response |
|---|---|
| Prompt injection asks for wallet secret | Never expose signer/secret; reject secret access |
| Model suggests unauthorized transfer | Treat as proposal; QPC Policy Gate rejects unless independently authorized |
| Gateway signature substituted for provider TEE | Reject under TEE-required policy |
| Expired or replayed attestation | Reject |
| Response changed after signature verification | Reject; bind exact bytes |
| Provider outage or timeout | Fail closed; no silent fallback |
| Malicious provider returns malformed structured output | Schema validation and reject |
| API key appears in provider error | Redact before logs or returned error |
| Provider requests raw Zcash UA/viewing key | Do not disclose; reject or use a separately reviewed private channel |
| Provider capability declaration lies | Do not treat capability metadata as proof; verify evidence |

## Operational requirements

- Pin dependencies and review transitive dependency changes.
- Keep secrets outside the repository and test fixtures.
- Use bounded timeouts, retries, payload sizes, and response sizes.
- Record only minimum non-sensitive audit metadata; never persist raw prompts or responses by default.
- Provide provider revocation and model allowlist controls.
- Adapter construction fails unless an API key, exact model allowlist, and non-empty measured compose-hash allowlist are supplied. Production activation must additionally require independently reviewed values and live verification.
- No production activation based solely on a successful mock test or provider marketing claim.
- The adapter has no signer or payment methods; integration code must preserve that separation and route any proposed action through the independent QPC Policy Gate.
