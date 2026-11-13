import test from 'node:test';
import assert from 'node:assert/strict';
import { AIProvider } from '../src/provider.mjs';
import { createAgentRuntimeAI } from '../src/runtime.mjs';

class FakeProvider extends AIProvider {
  constructor(result = { providerId: 'fake', model: 'fake/model', text: 'Suggested action: send funds.' }) {
    super();
    this.result = result;
    this.calls = 0;
  }
  async getCapabilities() { return { providerId: 'fake', supportedModels: ['fake/model'] }; }
  async verifyDeployment() { return { valid: true }; }
  async generate() { this.calls++; return this.result; }
}

test('returns AI output only as explicitly untrusted advice', async () => {
  const provider = new FakeProvider();
  const runtimeAI = createAgentRuntimeAI({ provider });
  const result = await runtimeAI.requestAdvice({ prompt: 'What should I do?' });
  assert.equal(provider.calls, 1);
  assert.equal(result.kind, 'untrusted_ai_advice');
  assert.equal(result.trust, 'untrusted');
  assert.equal(result.paymentAuthorization, 'none');
  assert.equal(result.actionExecution, 'not_available');
  assert.equal('execute' in result, false);
  assert.equal('sign' in result, false);
  assert.equal('wallet' in result, false);
});

test('rejects objects that do not implement the QPC AIProvider contract', () => {
  assert.throws(() => createAgentRuntimeAI({ provider: { generate: async () => ({ text: 'hi' }) } }), /invalid_ai_provider/);
});

test('refuses wallet and signer capabilities at construction', () => {
  const provider = new FakeProvider();
  assert.throws(() => createAgentRuntimeAI({ provider, signer: {} }), /wallet_capability_forbidden/);
  assert.throws(() => createAgentRuntimeAI({ provider, wallet: {} }), /wallet_capability_forbidden/);
  assert.throws(() => createAgentRuntimeAI({ provider, signAndSendTransaction() {} }), /wallet_capability_forbidden/);
});

test('does not leak provider exception details across the runtime boundary', async () => {
  class FailingProvider extends FakeProvider {
    async generate() { throw new Error('api_key=super-secret prompt contents'); }
  }
  const runtimeAI = createAgentRuntimeAI({ provider: new FailingProvider() });
  await assert.rejects(runtimeAI.requestAdvice({ prompt: 'hello' }), (error) => {
    assert.match(error.message, /inference_failed/);
    assert.doesNotMatch(error.message, /super-secret|prompt contents/);
    return true;
  });
});

test('rejects malformed provider output instead of passing it to the agent', async () => {
  const runtimeAI = createAgentRuntimeAI({ provider: new FakeProvider({ text: 123 }) });
  await assert.rejects(runtimeAI.requestAdvice({ prompt: 'hello' }), /invalid_provider_response/);
});
