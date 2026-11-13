# QPC AI Provider — Initial Contract Test Plan

**Status:** test design only. These are acceptance cases, not executable tests. No AI provider source module or test harness is added in this stage.

Use deterministic fake providers, fixed clocks, fixed byte fixtures, and injected verifier functions. Tests must not call live inference endpoints or require API keys.

| ID | Scenario | Expected result |
|---|---|---|
| T01 | Provider reports valid capabilities | Accept capability shape; do not treat it as attestation |
| T02 | Requested model is not allowlisted/supported | Reject before network inference |
| T03 | Required privacy class is unsupported | Reject; no downgrade |
| T04 | Required deployment attestation is invalid | Fail closed; inference not called |
| T05 | Deployment attestation is expired | Fail closed; inference not called |
| T06 | Model identity or signer differs from attested deployment | Fail closed |
| T07 | Policy requires provider TEE but only gateway signature exists | Reject |
| T08 | Valid provider-TEE evidence matches model and signer | Continue to response verification |
| T09 | Response signature is missing while required | Reject |
| T10 | Response bytes are modified after signing | Reject |
| T11 | Signature is valid for a different request | Reject due to request/response binding |
| T12 | Provider times out or is unavailable | Return bounded error; no silent provider fallback |
| T13 | Fallback is explicitly configured to a separately approved provider | Use only that exact provider after its own checks |
| T14 | Model asks runtime to transfer funds | Output remains advisory; QPC Policy Gate still decides |
| T15 | Adapter is inspected for wallet signer access | No signer/key capability exists in its interface |
| T16 | Provider error includes API key or secret | Redact secret before logs and returned diagnostics |
| T17 | Provider returns malformed/oversized output | Reject within configured bounds |
| T18 | A second provider implements the interface | Core policy tests pass without provider-specific branches |

## Test seams required before implementation

- Fake provider with configurable capabilities, responses, errors, and call counters.
- Fixed clock for attestation freshness and expiry.
- Injected deployment-attestation verifier and response-signature verifier.
- Deterministic canonical request/response byte fixtures.
- Spy asserting inference is not called when preflight verification fails.
- Spy proving no fallback occurs by default.
- Type/API boundary test proving the adapter receives no wallet signer.

## Test execution policy

When implementation begins, place executable tests in the repository's established Node.js test convention only after confirming the chosen runtime and module format. Keep AI provider tests isolated from `agent-router` payment tests. Do not add a dependency or claim tests pass until a real test command is run and its output is recorded.

## Release gate

All cases T01–T18 must pass before enabling a provider adapter. Independent review must additionally inspect the official SDK verification path, trust roots, prompt/data leakage, error redaction, and the boundary between AI suggestions and payment authorization.
