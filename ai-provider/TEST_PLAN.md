# QPC AI Provider — Contract Tests and Coverage

**Status:** initial executable offline contract suite implemented. Live NEAR AI integration and full acceptance remain gated.

Run from `ai-provider/` with Node.js `>=24.22.1`:

```sh
npm test
```

The suite uses a deterministic fake SDK client. It does not call NEAR AI, require a real credential, or prove real attestation correctness.

## Executable coverage

| Scenario | Expected result | Test status |
|---|---|---|
| Missing API key | Fail closed | PASS |
| Missing model allowlist | Fail closed | PASS |
| Missing compose-hash allowlist | Fail closed | PASS |
| Unsupported model | Reject before provider call | PASS |
| Deployment compose hash outside operator allowlist | Reject before inference | PASS |
| No model TEE evidence (Incognito/Gateway-only) | Reject before inference | PASS |
| Gateway-only response receipt | Reject; do not return text | PASS |
| Deployment preflight and receipt verification order | Verify -> inference -> verify response | PASS |
| Provider error containing secret text | Return stable redacted error | PASS |
| Empty and oversized prompt | Reject | PASS |
| Wallet/signing surface | No signer or transaction methods; adapter implements provider-neutral base class | PASS |
| SDK security configuration | E2EE on, attestation cache TTL zero, deployment verifier and model policy supplied | PASS |
| SDK model report without verified deployment provenance | Reject before inference | PASS |
| Invalid response receipt | Reject without returning response text | PASS |

## Additional executable Agent Runtime boundary coverage

Five tests in `test/runtime.test.mjs` verify that output is advisory-only, invalid providers are rejected, wallet/signer capabilities are refused, provider errors are redacted, and malformed output is rejected.

## Remaining acceptance work

- Exact signed request/response byte mutation and cross-request replay regression fixtures. These are delegated to the pinned official SDK but have not been independently tested here.
- Attestation expiry/revocation and model/signer mismatch fixtures.
- Provider timeout/cancellation behavior and explicit retry guarantees.
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
