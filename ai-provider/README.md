# QPC AI Provider

Off-chain, provider-neutral inference boundary. The NEAR AI Cloud adapter is optional and is not part of QPC payment settlement.

## Security behavior

- Requires an explicit model allowlist (`QPC_NEARAI_ALLOWED_MODELS`).
- Requires an explicit deployment compose-hash allowlist (`QPC_NEARAI_ALLOWED_COMPOSE_HASHES`).
- Requires NEAR model TEE attestation and E2EE (`e2ee: true`); Incognito/Gateway-only deployments are rejected.
- Re-verifies deployment before every inference and verifies the response receipt before returning text.
- Accepts only `provider_tee` response signatures. Gateway signatures alone are rejected.
- No fallback, wallet signer, transaction builder, or payment API is exposed.
- Provider errors are replaced with stable redacted error codes.

This adapter does not authorize actions. Agent Runtime must treat returned text as untrusted advisory data and independently apply QPC Policy Gate before any action.

## Runtime requirements

Node.js `>=24.22.1` is required by the pinned `@nearai/inference-sdk@0.3.0` package. The repository's existing `agent-router` package remains separate and does not inherit this dependency.

## Configuration

Supply credentials through the runtime secret manager/environment. Never commit `.env` files or API keys.

```sh
export NEARAI_API_KEY='...'
export QPC_NEARAI_MODEL='your-provider-tee-model-id'
export QPC_NEARAI_ALLOWED_MODELS='your-provider-tee-model-id'
export QPC_NEARAI_ALLOWED_COMPOSE_HASHES='approved-compose-hash-1,approved-compose-hash-2'
# Optional; defaults to https://cloud-api.near.ai/v1/
export NEARAI_BASE_URL='https://cloud-api.near.ai/v1/'
```

The deployment hashes must be reviewed and supplied by the operator from independently approved deployment evidence. There is deliberately no permissive default. A model name alone is not an approval.

## Usage

```js
import { createNearAIProvider } from './src/index.mjs';

const provider = createNearAIProvider();
const result = await provider.generate({ prompt: 'Summarize this public information.' });
// result.text is returned only after a provider-TEE response receipt verifies.
console.log(result.text);
```

The prompt must not contain wallet secrets, private keys, Zcash viewing keys, raw Unified Addresses, or unnecessary personal/payment metadata.

## Tests

```sh
npm test
```

Tests use a fake SDK client and do not make live requests. They do not prove NEAR AI's infrastructure is secure. Live integration requires a separately supplied key, an approved model, and independently reviewed deployment measurements.
