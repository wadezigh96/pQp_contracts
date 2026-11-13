import { createPublicKey, KeyObject, verify as verifyEd25519 } from 'node:crypto';
import { ml_dsa65 } from '@noble/post-quantum/ml-dsa.js';
import { canonicalizeCrossL1AuthorizationEnvelope, computeCrossL1AuthorizationCommitment } from '../src/cross-l1-envelope.mjs';

export const HYBRID_SUITE = 'qpc-hybrid-ed25519-ml-dsa-65-v1';
const DOMAIN = 'QPC_HYBRID_AGENT_AUTHORIZATION_V1\0';
const identityKey = identity => JSON.stringify([identity.namespace, identity.id, identity.keyId]);
const plain = value => value && Object.getPrototypeOf(value) === Object.prototype;
function bytes(value, length) {
  if (!(value instanceof Uint8Array) || value.byteLength !== length) throw new Error('invalid signature or public key length');
  return Uint8Array.from(value);
}
function epoch(value) {
  if (!Number.isSafeInteger(value) || value < 1) throw new Error('positive key epoch required');
  return value;
}

/** Exact bytes signed by BOTH algorithms. No caller-selected algorithm or digest. */
export function hybridAuthorizationMessage(input, keyEpoch) {
  const envelope = canonicalizeCrossL1AuthorizationEnvelope(input);
  return Buffer.from(DOMAIN + JSON.stringify([
    HYBRID_SUITE, epoch(keyEpoch), envelope.agentIdentity.namespace,
    envelope.agentIdentity.id, envelope.agentIdentity.keyId,
    computeCrossL1AuthorizationCommitment(envelope),
  ]), 'utf8');
}

/**
 * Operator-pinned, immutable key snapshot. Pass the returned function as the
 * existing verifyAgentAuthorization hook. Signatures are supplied by lookup;
 * lookup is untrusted and cannot supply keys, algorithm choice or trust roots.
 * Rotation/revocation requires a newly configured verifier. This is OFF-CHAIN
 * authentication only; no classical chain-owner bypass is removed by it.
 */
export function createHybridAgentVerifier({ trustedKeys, lookupProof, lookupTimeoutMs = 5000 }) {
  if (!Array.isArray(trustedKeys) || !trustedKeys.length || typeof lookupProof !== 'function') {
    throw new Error('operator trusted keys and proof lookup required');
  }
  if (!Number.isSafeInteger(lookupTimeoutMs) || lookupTimeoutMs < 1 || lookupTimeoutMs > 30000) {
    throw new Error('lookup timeout must be 1..30000 ms');
  }
  const keys = new Map();
  for (const entry of trustedKeys) {
    if (!plain(entry) || !plain(entry.agentIdentity) ||
        Object.keys(entry.agentIdentity).sort().join(',') !== 'id,keyId,namespace' ||
        Object.values(entry.agentIdentity).some(v => typeof v !== 'string' || !v.length || v.length > 256)) {
      throw new Error('invalid trusted identity');
    }
    const id = identityKey(entry.agentIdentity);
    if (keys.has(id)) throw new Error('duplicate trusted identity');
    const inputKey = entry.ed25519PublicKey;
    const isPublicObject = inputKey instanceof KeyObject && inputKey.type === 'public';
    const isPublicPem = typeof inputKey === 'string' &&
      /^-----BEGIN PUBLIC KEY-----\r?\n[A-Za-z0-9+/=\r\n]+-----END PUBLIC KEY-----\s*$/.test(inputKey);
    if (!isPublicObject && !isPublicPem) throw new Error('Ed25519 public key required, never a private key');
    const ed25519 = isPublicObject ? inputKey : createPublicKey(inputKey);
    if (ed25519.asymmetricKeyType !== 'ed25519') throw new Error('Ed25519 public key required');
    keys.set(id, { keyEpoch: epoch(entry.keyEpoch), ed25519,
      mlDsa65: bytes(entry.mlDsa65PublicKey, ml_dsa65.lengths.publicKey) });
  }
  return async function verifyAgentAuthorization({ envelope: input, authorizationCommitment }) {
    try {
      const envelope = canonicalizeCrossL1AuthorizationEnvelope(input);
      const commitment = computeCrossL1AuthorizationCommitment(envelope);
      if (authorizationCommitment !== commitment) return false;
      const key = keys.get(identityKey(envelope.agentIdentity));
      if (!key) return false;
      const controller = new AbortController();
      let timer;
      let proof;
      try {
        proof = await Promise.race([
          Promise.resolve().then(() => lookupProof(Object.freeze({ authorizationCommitment: commitment,
            agentIdentity: envelope.agentIdentity, suite: HYBRID_SUITE, keyEpoch: key.keyEpoch }),
          { signal: controller.signal })),
          new Promise(resolve => { timer = setTimeout(() => { controller.abort(); resolve(null); }, lookupTimeoutMs); }),
        ]);
      } finally {
        clearTimeout(timer);
      }
      if (!plain(proof) || Object.keys(proof).sort().join(',') !== 'ed25519Signature,keyEpoch,mlDsa65Signature,suite' ||
          proof.suite !== HYBRID_SUITE || proof.keyEpoch !== key.keyEpoch) return false;
      const classical = bytes(proof.ed25519Signature, 64);
      const pq = bytes(proof.mlDsa65Signature, ml_dsa65.lengths.signature);
      const message = hybridAuthorizationMessage(envelope, key.keyEpoch);
      return verifyEd25519(null, message, key.ed25519, classical) &&
        ml_dsa65.verify(pq, message, key.mlDsa65);
    } catch {
      // Missing, malformed, unknown, revoked or failed evidence fails closed.
      return false;
    }
  };
}
