# QPC AI Provider — Contract Tests and Coverage

**Status:** executable offline test files are implemented. Their execution in this environment has not yet been independently observed; GitHub Actions has been configured, but no run result is currently available. Live NEAR AI integration and full acceptance remain gated.

Run from `ai-provider/` with Node.js `>=24.22.1`:

```sh
npm test
```

The suite uses a deterministic fake SDK client. It does not call NEAR AI, require a real credential, or prove real attestation correctness.

## Implemented test coverage (execution not yet confirmed)

| Scenario | Expected result | Test status |
|---|---|---|
| Missing API key | Fail closed | Implemented; execution pending |
| Missing model allowlist | Fail closed | Implemented; execution pending |
| Missing compose-hash allowlist | Fail closed | Implemented; execution pending |
| Unsupported model | Reject before provider call | Implemented; execution pending |
| Deployment compose hash outside operator allowlist | Reject before inference | Implemented; execution pending |
| No model TEE evidence (Incognito/Gateway-only) | Reject before inference | Implemented; execution pending |
| Gateway-only response receipt | Reject; do not return text | Implemented; execution pending |
| Deployment preflight and receipt verification order | Verify -> inference -> verify response | Implemented; execution pending |
| Provider error containing secret text | Return stable redacted error | Implemented; execution pending |
| Empty and oversized prompt | Reject | Implemented; execution pending |
| Wallet/signing surface | No signer or transaction methods; adapter implements provider-neutral base class | Implemented; execution pending |
| SDK security configuration | E2EE on, attestation cache TTL zero, deployment verifier and model policy supplied | Implemented; execution pending |
| SDK model report without verified deployment provenance | Reject before inference | Implemented; execution pending |
| Invalid response receipt | Reject without returning response text | Implemented; execution pending |
| Agent Runtime labels output untrusted | No action authorization or execution | Implemented; execution pending |
| Invalid provider contract | Reject construction | Implemented; execution pending |
| Wallet/signer capability supplied | Reject construction | Implemented; execution pending |
| Provider exception | Redact details | Implemented; execution pending |
| Malformed provider output | Reject | Implemented; execution pending |
| Inference request timeout | Pass bounded per-request timeout to SDK | Implemented; execution pending |
| Oversized returned text | Reject above configured character cap | Implemented; execution pending |
| Invalid timeout/response cap configuration | Reject during construction | Implemented; execution pending |

## Additional executable Agent Runtime boundary coverage

Five tests in `test/runtime.test.mjs` verify that output is advisory-only, invalid providers are rejected, wallet/signer capabilities are refused, provider errors are redacted, and malformed output is rejected.

## Remaining acceptance work

- Exact signed request/response byte mutation and cross-request replay regression fixtures. These are delegated to the pinned official SDK but have not been independently tested here.
- Attestation expiry/revocation and model/signer mismatch fixtures.
- Abort/cancellation behavior for provider timeouts and explicit retry guarantees (automatic retries remain disabled by the SDK).
- Malformed/oversized response behavior beyond the configured completion-token limit.
- Second-provider conformance and explicit fallback-policy tests.
- Full QPC Policy Gate integration proving model output cannot authorize a payment.
- Live end-to-end test using an approved TEE model and independently reviewed compose hash.

## Test seams

- Fake SDK client injected through `clientFactory`.
- Fixed clock injected through `now`.
- Deployment measurement allowlist supplied by test environment.
- SDK verifier remains the cryptographic implementation; production integration must pin and review its trust roots and verification path.

## Release gate

Do not enable this adapter in a production agent until remaining acceptance work, live E2E verification with an approved model, dependency review, and independent security review are complete. A passing mock suite does not prove provider infrastructure is secure.
