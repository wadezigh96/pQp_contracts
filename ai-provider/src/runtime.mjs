import { AIProvider } from './provider.mjs';

function fail(code) {
  const error = new Error(`QPC Agent Runtime AI rejected request (${code})`);
  error.code = code;
  return error;
}

/**
 * Create the AI inference port consumed by an Agent Runtime.
 *
 * This is intentionally advisory-only: it exposes no wallet, signer,
 * transaction, policy mutation, tool execution, or payment authorization API.
 * The QPC Policy Gate and chain wallet adapters remain separate.
 */
export function createAgentRuntimeAI(options = {}) {
  if (!options || typeof options !== 'object') throw new TypeError('runtime options must be an object');

  for (const forbidden of ['wallet', 'signer', 'privateKey', 'mnemonic', 'signAndSendTransaction', 'transactionSigner']) {
    if (Object.hasOwn(options, forbidden)) throw fail('wallet_capability_forbidden');
  }

  const { provider } = options;
  if (!(provider instanceof AIProvider)) throw fail('invalid_ai_provider');

  return Object.freeze({
    async requestAdvice(request) {
      let response;
      try {
        response = await provider.generate(request);
      } catch {
        // Do not pass provider exceptions, credentials, or prompt fragments across the runtime boundary.
        throw fail('inference_failed');
      }

      if (!response || typeof response.text !== 'string') throw fail('invalid_provider_response');

      return Object.freeze({
        kind: 'untrusted_ai_advice',
        providerId: response.providerId,
        model: response.model,
        text: response.text,
        trust: 'untrusted',
        paymentAuthorization: 'none',
        actionExecution: 'not_available',
      });
    },
  });
}
