# QPC AI Provider Interface — Specification v1

**Status:** design-only. No provider adapter, model runtime, attestation verifier, or inference path is implemented by this document.

## 1. Purpose and boundary

This module is an off-chain AI inference integration boundary for QPC Agent Runtime. It is separate from the cross-chain payment router and from every chain-specific payment adapter.

`Agent Runtime -> AI Provider Interface -> provider adapter -> inference service`

The AI provider may return suggestions, structured data, or proposed actions. Its output is untrusted input. It MUST NOT authorize, sign, construct, submit, or broadcast a payment. All payment actions remain subject to the existing QPC Policy Gate and the relevant chain/wallet authorization boundary.

The adapter MUST NOT receive seed phrases, private keys, spending keys, wallet signer objects, or unrestricted transaction capabilities.

## 2. Initial scope

- Provider-neutral interface and capability discovery.
- Model allowlisting and policy-based privacy selection.
- Optional deployment/TEE evidence verification.
- Exact request/response binding for supported response signatures.
- Fail-closed behavior for required verification failures.
- Deterministic tests with mocked providers and cryptographic verification seams.

Out of scope: autonomous payments, wallet signing, tool execution, cross-chain settlement, production secrets management, and claims of end-to-end anonymity.

## 3. Core types (language-neutral contract)

```ts
type ExecutionTrust =
  | "unverified"
  | "gateway_signed"
  | "provider_tee_attested";

type PrivacyClass =
  | "public"
  | "confidential"
  | "private_tee";

type ProviderCapabilities = {
  providerId: string;
  supportedModels: string[];
  privacyClasses: PrivacyClass[];
  executionTrustLevels: ExecutionTrust[];
  supportsDeploymentAttestation: boolean;
  responseSignatureTypes: Array<"provider_tee" | "gateway" | "none">;
};

type VerificationResult = {
  valid: boolean;
  trustLevel: ExecutionTrust;
  model?: string;
  signer?: string;
  issuedAt?: number;
  expiresAt?: number;
  reasonCode?: string;
};

interface AIProvider {
  getCapabilities(): Promise<ProviderCapabilities>;
  verifyDeployment(model: string): Promise<VerificationResult>;
  generate(request: AIRequest): Promise<AIResponse>;
  verifyResponse(
    request: AIRequest,
    response: AIResponse
  ): Promise<VerificationResult>;
}
```

These are conceptual types, not an implemented or frozen public API. Before implementation, define canonical serialization, byte encodings, error codes, time source, and signature verification APIs.

## 4. Policy requirements

1. The runtime selects an explicitly allowlisted provider and model.
2. The request declares its required privacy class and minimum execution trust.
3. The adapter checks advertised capabilities; a provider's self-description is not attestation.
4. If policy requires TEE execution, verify fresh deployment evidence bound to the selected model and expected signer before inference.
5. Send inference only after policy checks pass.
6. Verify any required response signature against the exact canonical request and response bytes, including the correct signer and domain.
7. Return output only after every required verification succeeds.
8. Any missing, invalid, expired, mismatched, or unverifiable evidence causes rejection. No silent downgrade.
9. Provider outage, timeout, rate limit, or verification failure MUST NOT trigger a different provider/model unless a separately configured fallback policy explicitly permits that exact alternative and its trust requirements are independently met. Default fallback is disabled.
10. AI output remains advisory. The QPC Policy Gate independently evaluates every proposed action.

## 5. NEAR AI adapter notes

The NEAR AI integration is an optional provider adapter, not a dependency of QPC payment settlement. Its OpenAI-compatible inference endpoint does not by itself establish TEE execution or response authenticity.

The adapter must use the official inference SDK's supported verification path rather than inventing a custom signature format. Preserve the distinction between:

- `provider_tee`: signature binding inference bytes to the attested model/provider TEE signer.
- `gateway`: signature binding bytes to the attested gateway signer.

A gateway signature alone MUST NOT satisfy a policy that requires proof of model execution inside a provider TEE. Deployment evidence must be checked separately and bound to the selected model and signer. Exact SDK API names and evidence formats must be verified against the pinned SDK version before implementation.

Do not conflate NEAR AI Gateway desktop session tooling with the NEAR AI Cloud inference API. Gateway desktop tooling is not a QPC bridge, wallet, or payment rail.

## 6. API and data handling

- No API keys in source, tests, logs, error messages, fixtures, or committed configuration.
- Credentials are supplied at runtime through a secret store/environment boundary and redacted before diagnostics.
- Minimize prompts and context; do not send wallet secrets, raw Zcash Unified Addresses, viewing keys, or unnecessary personal/payment metadata.
- Explicitly document provider retention, training, region, and privacy mode for each configured model.
- Do not infer privacy guarantees from labels such as "private" or "incognito"; require evidence matching the configured policy.
- Bound request size, response size, timeout, and retry count.
- Treat model output, tool calls, citations, and structured JSON as untrusted data. Validate schema and authorization independently.

## 7. Versioning and acceptance

The interface is not implementation-ready until:
- exact SDK version and verification APIs are pinned;
- canonical request/response bytes and signature domains are specified;
- provider/model identity and attestation freshness rules are specified;
- policy and error semantics are agreed;
- contract tests in `TEST_PLAN.md` pass against a deterministic fake provider;
- an independent security review covers prompt/data leakage and the signer boundary.

Passing adapter tests does not prove the provider is secure, that a TEE is uncompromised, or that the overall agent is safe.
