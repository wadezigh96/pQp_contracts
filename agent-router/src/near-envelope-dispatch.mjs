import {
  canonicalizeCrossL1AuthorizationEnvelope,
  computeCrossL1AuthorizationCommitment,
} from './cross-l1-envelope.mjs';

/** Prepare the actual full-envelope NEAR authorization call; never sign/send it.
 * The contract authenticates predecessor and owner profile. This unsigned data
 * does not prove authorization, destination execution, or settlement.
 */
export function buildNearEnvelopeAuthorizationDispatch({ envelope: input, nowUnix } = {}) {
  const envelope = canonicalizeCrossL1AuthorizationEnvelope(input);
  const source = envelope.source;
  if (envelope.agentIdentity.namespace !== 'near' ||
      !['mainnet', 'testnet'].includes(source.nativeChainId) ||
      source.networkId !== `near:${source.nativeChainId}`) throw new Error('NEAR source network mismatch');
  const account = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
  if (source.adapterId.length < 2 || source.adapterId.length > 64 || !account.test(source.adapterId)) {
    throw new Error('invalid NEAR source contract account');
  }
  if (source.accountId !== envelope.agentIdentity.id || source.accountId !== envelope.agentIdentity.keyId ||
      source.accountId.length < 2 || source.accountId.length > 64 || !account.test(source.accountId)) {
    throw new Error('NEAR source agent identity mismatch');
  }
  if (!Number.isSafeInteger(nowUnix) || nowUnix < envelope.time.notBeforeUnix || nowUnix >= envelope.time.expiresAtUnix) {
    throw new Error('authorization outside valid time window');
  }
  if (nowUnix < envelope.time.issuedAtUnix) throw new Error('authorization issued in future');
  if (envelope.replayDomain.authorityId !== source.adapterId) throw new Error('NEAR replay authority mismatch');
  return Object.freeze({
    state: 'PREPARED_AUTHORIZATION_ONLY',
    execution: false,
    qpcCall: Object.freeze({
      receiverId: source.adapterId,
      methodName: 'authorize_envelope',
      args: Object.freeze({ envelope, authorization_commitment: computeCrossL1AuthorizationCommitment(envelope) }),
      gas: '100000000000000', // Local suggestion, not production gas certification.
      deposit: '0',
    }),
  });
}
