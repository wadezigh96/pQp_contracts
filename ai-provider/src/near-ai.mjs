import { validateAIRequest } from './provider.mjs';

const DEFAULT_BASE_URL = 'https://cloud-api.near.ai/v1/';
const DEFAULT_MAX_COMPLETION_TOKENS = 2_048;

function parseList(value) {
  return new Set(String(value ?? '').split(',').map((item) => item.trim()).filter(Boolean));
}

function safeFailure(code) {
  // Never propagate provider/SDK error text: it may contain credentials or prompts.
  const error = new Error(`NEAR AI request rejected (${code})`);
  error.code = code;
  return error;
}

async function defaultClientFactory(options) {
  // Dynamic import keeps offline contract tests independent from the SDK package.
  const { InferenceClient } = await import('@nearai/inference-sdk/node');
  return new InferenceClient(options);
}

/**
 * NEAR AI Cloud adapter. It requires model TEE attestation and a local
 * deployment-measurement allowlist. Incognito/Gateway-only models are rejected.
 * @param {Object} options
 * @param {NodeJS.ProcessEnv|Record<string,string|undefined>} [options.env]
 * @param {(sdkOptions:Object)=>Promise<Object>|Object} [options.clientFactory]
 * @param {()=>number} [options.now]
 * @param {number} [options.maxPromptChars]
 * @param {number} [options.maxCompletionTokens]
 */
export function createNearAIProvider({
  env = process.env,
  clientFactory = defaultClientFactory,
  now = Date.now,
  maxPromptChars = 16_000,
  maxCompletionTokens = DEFAULT_MAX_COMPLETION_TOKENS,
} = {}) {
  const apiKey = env.NEARAI_API_KEY;
  const model = env.QPC_NEARAI_MODEL;
  const allowedModels = parseList(env.QPC_NEARAI_ALLOWED_MODELS);
  const allowedComposeHashes = parseList(env.QPC_NEARAI_ALLOWED_COMPOSE_HASHES);
  const baseUrl = env.NEARAI_BASE_URL || DEFAULT_BASE_URL;

  if (!apiKey) throw safeFailure('configuration_missing_api_key');
  if (!model || !allowedModels.has(model)) throw safeFailure('model_not_allowlisted');
  if (allowedComposeHashes.size === 0) throw safeFailure('deployment_allowlist_missing');
  let clientPromise;
  let verifiedDeployment;

  async function getClient() {
    if (!clientPromise) {
      clientPromise = Promise.resolve(clientFactory({
        apiKey,
        baseUrl,
        e2ee: true,
        ohttp: false,
        signingAlgo: 'ed25519',
        // Force a fresh preflight instead of trusting a long-lived cached attestation.
        attestationCacheTimeToLiveMs: 0,
        responseCacheTimeToLiveMs: 5 * 60 * 1_000,
        modelVerification: {
          verifiers: {
            // This callback runs after the SDK authenticates the measured deployment.
            // Its success marks deploymentProvenance as verified in the SDK result.
            deployment: (deployment) => {
              const composeHash = deployment?.runtimeMeasurements?.composeHash;
              if (typeof composeHash !== 'string' || !allowedComposeHashes.has(composeHash))
                throw safeFailure('deployment_measurement_not_allowlisted');
            },
          },
        },
        // Additional SDK policy receives the model ID and runs after the verifier above.
        deploymentPolicy: ({ model: requestedModel }) => {
          if (requestedModel !== model || !allowedModels.has(requestedModel))
            throw safeFailure('deployment_model_mismatch');
        },
      })).catch(() => { clientPromise = undefined; throw safeFailure('sdk_initialization_failed'); });
    }
    return clientPromise;
  }

  async function verifyDeployment(requestedModel = model) {
    if (requestedModel !== model || !allowedModels.has(requestedModel))
      throw safeFailure('model_not_allowlisted');
    const client = await getClient();
    let result;
    try { result = await client.verify(requestedModel); }
    catch { verifiedDeployment = undefined; throw safeFailure('deployment_verification_failed'); }
    if (!Array.isArray(result?.models) || result.models.length === 0) {
      verifiedDeployment = undefined;
      throw safeFailure('model_tee_attestation_missing');
    }
    if (result.models.some((entry) => entry?.deploymentProvenance !== 'verified')) {
      verifiedDeployment = undefined;
      throw safeFailure('deployment_provenance_unverified');
    }
    verifiedDeployment = { model: requestedModel, verifiedAt: now(), models: result.models };
    return Object.freeze({
      valid: true,
      trustLevel: 'provider_tee_attested',
      model: requestedModel,
      verifiedAt: verifiedDeployment.verifiedAt,
      modelAttestationCount: result.models.length,
    });
  }

  return Object.freeze({
    async getCapabilities() {
      return Object.freeze({
        providerId: 'near-ai-cloud',
        supportedModels: Object.freeze([...allowedModels]),
        privacyClasses: Object.freeze(['private_tee']),
        executionTrustLevels: Object.freeze(['provider_tee_attested']),
        supportsDeploymentAttestation: true,
        responseSignatureTypes: Object.freeze(['provider_tee']),
        fallbackEnabled: false,
      });
    },

    verifyDeployment,

    async generate(request) {
      validateAIRequest(request, { maxPromptChars, maxCompletionTokens });
      if (request.model !== undefined && request.model !== model)
        throw safeFailure('model_not_allowlisted');
      // Preflight is repeated before every inference; no stale attestation reuse.
      await verifyDeployment(model);
      const client = await getClient();
      let completion;
      try {
        completion = await client.chat.completions.create({
          model,
          messages: [{ role: 'user', content: request.prompt }],
          max_completion_tokens: request.maxCompletionTokens ?? maxCompletionTokens,
        });
      } catch {
        throw safeFailure('inference_failed');
      }
      if (!completion || typeof completion.id !== 'string' || !completion.id)
        throw safeFailure('completion_id_missing');

      let receipt;
      try { receipt = await client.verifyResponse(completion.id); }
      catch { throw safeFailure('response_signature_invalid'); }
      if (receipt?.signatureKind !== 'provider_tee')
        throw safeFailure('provider_tee_signature_required');
      if (!verifiedDeployment || verifiedDeployment.model !== model)
        throw safeFailure('deployment_preflight_missing');

      const text = completion.choices?.[0]?.message?.content;
      if (typeof text !== 'string') throw safeFailure('response_content_invalid');
      return Object.freeze({
        providerId: 'near-ai-cloud',
        model,
        text,
        signatureKind: 'provider_tee',
        verifiedAt: now(),
      });
    },
  });
}
