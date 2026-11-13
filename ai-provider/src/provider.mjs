/** @typedef {'public'|'confidential'|'private_tee'} PrivacyClass */
/** @typedef {'unverified'|'gateway_signed'|'provider_tee_attested'} ExecutionTrust */

/**
 * @typedef {Object} AIRequest
 * @property {string} prompt
 * @property {string=} model
 * @property {number=} maxCompletionTokens
 */

/**
 * @typedef {Object} AIResponse
 * @property {string} providerId
 * @property {string} model
 * @property {string} text
 * @property {'provider_tee'} signatureKind
 * @property {number} verifiedAt
 */

/**
 * Provider-neutral contract. Implementations must return only verified output.
 * This interface deliberately contains no wallet, signer, or transaction methods.
 */
export class AIProvider {
  getCapabilities() { throw new Error('getCapabilities() must be implemented'); }
  verifyDeployment(_model) { throw new Error('verifyDeployment() must be implemented'); }
  generate(_request) { throw new Error('generate() must be implemented'); }
}

export function validateAIRequest(request, { maxPromptChars = 16_000, maxCompletionTokens = 2_048 } = {}) {
  if (!request || typeof request !== 'object') throw new TypeError('AI request must be an object');
  if (typeof request.prompt !== 'string' || request.prompt.trim().length === 0)
    throw new TypeError('AI prompt must be a non-empty string');
  if (request.prompt.length > maxPromptChars) throw new RangeError('AI prompt exceeds configured size limit');
  if (request.model !== undefined && (typeof request.model !== 'string' || request.model.length > 200))
    throw new TypeError('AI model identifier is invalid');
  if (request.maxCompletionTokens !== undefined &&
      (!Number.isInteger(request.maxCompletionTokens) || request.maxCompletionTokens < 1 || request.maxCompletionTokens > maxCompletionTokens))
    throw new RangeError('maxCompletionTokens is outside the allowed range');
}
