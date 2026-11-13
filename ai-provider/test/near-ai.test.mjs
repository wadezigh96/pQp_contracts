import test from 'node:test';
import assert from 'node:assert/strict';
import { createNearAIProvider } from '../src/near-ai.mjs';

const env = {
  NEARAI_API_KEY: 'test-secret-not-real',
  QPC_NEARAI_MODEL: 'vendor/model-tee',
  QPC_NEARAI_ALLOWED_MODELS: 'vendor/model-tee',
  QPC_NEARAI_ALLOWED_COMPOSE_HASHES: 'approved-compose-hash',
};

function fakeFactory({ signatureKind = 'provider_tee', models = [{ deploymentProvenance: 'verified' }],
  composeHash = 'approved-compose-hash', inferenceError = false } = {}) {
  const calls = [];
  const factory = async (options) => ({
    async verify(model) {
      calls.push(['verify', model]);
      options.deploymentPolicy({ model, deployment: { runtimeMeasurements: { composeHash } } });
      return { models };
    },
    chat: { completions: { create: async (input) => {
      calls.push(['generate', input]);
      if (inferenceError) throw new Error('secret-bearing fake provider error');
      return { id: 'completion-1', choices: [{ message: { content: 'verified answer' } }] };
    } } },
    async verifyResponse(id) {
      calls.push(['verifyResponse', id]);
      return { signatureKind };
    },
  });
  return { factory, calls };
}

test('capabilities expose only provider-TEE privacy and no fallback', async () => {
  const { factory } = fakeFactory();
  const provider = createNearAIProvider({ env, clientFactory: factory });
  const capabilities = await provider.getCapabilities();
  assert.deepEqual(capabilities.privacyClasses, ['private_tee']);
  assert.deepEqual(capabilities.responseSignatureTypes, ['provider_tee']);
  assert.equal(capabilities.fallbackEnabled, false);
});

test('fails closed when API key, model allowlist, or measurement allowlist is missing', () => {
  assert.throws(() => createNearAIProvider({ env: { ...env, NEARAI_API_KEY: '' }, clientFactory: fakeFactory().factory }), /configuration_missing_api_key/);
  assert.throws(() => createNearAIProvider({ env: { ...env, QPC_NEARAI_ALLOWED_MODELS: '' }, clientFactory: fakeFactory().factory }), /model_not_allowlisted/);
  assert.throws(() => createNearAIProvider({ env: { ...env, QPC_NEARAI_ALLOWED_COMPOSE_HASHES: '' }, clientFactory: fakeFactory().factory }), /deployment_allowlist_missing/);
});

test('rejects model not allowlisted before provider network activity', async () => {
  const { factory, calls } = fakeFactory();
  const provider = createNearAIProvider({ env, clientFactory: factory });
  await assert.rejects(provider.generate({ model: 'other/model', prompt: 'hello' }), /model_not_allowlisted/);
  assert.deepEqual(calls, []);
});

test('rejects deployment measurement outside local allowlist before inference', async () => {
  const { factory, calls } = fakeFactory({ composeHash: 'unapproved-hash' });
  const provider = createNearAIProvider({ env, clientFactory: factory });
  await assert.rejects(provider.generate({ prompt: 'hello' }), /deployment_verification_failed/);
  assert.equal(calls.some(([kind]) => kind === 'generate'), false);
});

test('rejects Incognito/Gateway-only model with no model TEE evidence', async () => {
  const { factory, calls } = fakeFactory({ models: [] });
  const provider = createNearAIProvider({ env, clientFactory: factory });
  await assert.rejects(provider.generate({ prompt: 'hello' }), /model_tee_attestation_missing/);
  assert.equal(calls.some(([kind]) => kind === 'generate'), false);
});

test('does not return answer when response receipt is Gateway-only', async () => {
  const { factory } = fakeFactory({ signatureKind: 'gateway' });
  const provider = createNearAIProvider({ env, clientFactory: factory });
  await assert.rejects(provider.generate({ prompt: 'hello' }), /provider_tee_signature_required/);
});

test('verifies deployment before inference and verifies response before returning text', async () => {
  const { factory, calls } = fakeFactory();
  let now = 1234;
  const provider = createNearAIProvider({ env, clientFactory: factory, now: () => now++ });
  const response = await provider.generate({ prompt: 'hello' });
  assert.equal(response.text, 'verified answer');
  assert.equal(response.signatureKind, 'provider_tee');
  assert.deepEqual(calls.map(([kind]) => kind), ['verify', 'generate', 'verifyResponse']);
});

test('redacts provider errors and never leaks configured API key', async () => {
  const { factory } = fakeFactory({ inferenceError: true });
  const provider = createNearAIProvider({ env, clientFactory: factory });
  await assert.rejects(provider.generate({ prompt: 'hello' }), (error) => {
    assert.match(error.message, /inference_failed/);
    assert.doesNotMatch(error.message, /test-secret-not-real/);
    assert.equal(error.cause, undefined);
    return true;
  });
});

test('rejects empty and oversized prompts', async () => {
  const { factory } = fakeFactory();
  const provider = createNearAIProvider({ env, clientFactory: factory, maxPromptChars: 4 });
  await assert.rejects(provider.generate({ prompt: '   ' }), /non-empty/);
  await assert.rejects(provider.generate({ prompt: '12345' }), /size limit/);
});

test('adapter has no wallet or transaction signing surface', () => {
  const { factory } = fakeFactory();
  const provider = createNearAIProvider({ env, clientFactory: factory });
  assert.deepEqual(Object.keys(provider).sort(), ['generate', 'getCapabilities', 'verifyDeployment']);
});
