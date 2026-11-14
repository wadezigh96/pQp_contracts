import test from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
import { readFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FileIntentStore } from '../../src/intent-store.mjs';
import { ml_dsa65 } from '@noble/post-quantum/ml-dsa.js';
import { HYBRID_SUITE, hybridAuthorizationMessage, createHybridAgentVerifier } from '../index.mjs';
import { authorizeCrossL1Envelope, computeCrossL1AuthorizationCommitment } from '../../src/cross-l1-envelope.mjs';

// Ephemeral sandbox keys only; never persisted, logged or sent. A rejection
// test serializes a dummy private key in memory to prove config refuses it.
const classical = generateKeyPairSync('ed25519');
const pq = ml_dsa65.keygen();
const fixture = JSON.parse(await readFile(new URL('../../../docs/fixtures/cross-l1-v1.json', import.meta.url)));
const envelope = fixture.envelope;
const trust = () => [{ agentIdentity: { ...envelope.agentIdentity }, keyEpoch: 1,
  ed25519PublicKey: classical.publicKey.export({ type: 'spki', format: 'pem' }),
  mlDsa65PublicKey: Uint8Array.from(pq.publicKey) }];
function proof(input = envelope, keyEpoch = 1) {
  const message = hybridAuthorizationMessage(input, keyEpoch);
  return { suite: HYBRID_SUITE, keyEpoch,
    ed25519Signature: sign(null, message, classical.privateKey),
    mlDsa65Signature: ml_dsa65.sign(message, pq.secretKey) };
}
const good = proof();
const request = input => ({ envelope: input, authorizationCommitment: computeCrossL1AuthorizationCommitment(input) });
const verifyWith = (p, trustedKeys = trust()) => createHybridAgentVerifier({ trustedKeys, lookupProof: async () => p });

test('valid AND-hybrid signatures authorize through existing hook and durable replay seam', async () => {
  let consumed = false;
  const result = await authorizeCrossL1Envelope(envelope, {
    nowUnix: envelope.time.issuedAtUnix + 1,
    verifyAgentAuthorization: verifyWith(good),
    consumeReplay: async () => { if (consumed) return false; consumed = true; return true; },
  });
  assert.equal(result.state, 'AUTHORIZED');
  await assert.rejects(authorizeCrossL1Envelope(envelope, {
    nowUnix: envelope.time.issuedAtUnix + 1, verifyAgentAuthorization: verifyWith(good),
    consumeReplay: async () => false,
  }), /already consumed/);
});

for (const [name, mutate] of [
  ['classical signature corrupted', p => { p.ed25519Signature[0] ^= 1; }],
  ['PQ signature corrupted', p => { p.mlDsa65Signature[0] ^= 1; }],
  ['classical-only downgrade', p => { delete p.mlDsa65Signature; }],
  ['PQ-only downgrade', p => { delete p.ed25519Signature; }],
  ['algorithm downgrade', p => { p.suite = 'ed25519'; }],
  ['old key epoch', p => { p.keyEpoch = 0; }],
  ['future key epoch', p => { p.keyEpoch = 2; }],
  ['truncated PQ signature', p => { p.mlDsa65Signature = p.mlDsa65Signature.slice(1); }],
  ['oversized signature', p => { p.ed25519Signature = new Uint8Array(65); }],
  ['caller trust injection', p => { p.publicKey = pq.publicKey; }],
]) {
  test(name + ' fails closed without consuming replay', async () => {
    const p = structuredClone(good); mutate(p);
    let consumed = false;
    await assert.rejects(authorizeCrossL1Envelope(envelope, {
      nowUnix: envelope.time.issuedAtUnix + 1, verifyAgentAuthorization: verifyWith(p),
      consumeReplay: async () => { consumed = true; return true; },
    }), /verification failed/);
    assert.equal(consumed, false);
  });
}
for (const [name, change] of [
  ['amount', e => { e.amount.sourceAtomic = '9'; }],
  ['recipient', e => { e.destination.recipientCommitment = 'c'.repeat(64); }],
  ['nonce', e => { e.nonce = '123'; }],
  ['policy', e => { e.policy.version++; }],
  ['expiry', e => { e.time.expiresAtUnix++; }],
  ['identity', e => { e.agentIdentity.keyId = 'other'; }],
  ['chain', e => { e.destination.nativeChainId = 'other'; }],
]) {
  test('signatures bind ' + name, async () => {
    const e = structuredClone(envelope); change(e);
    assert.equal(await verifyWith(good)(request(e)), false);
  });
}
test('proof lookup exceptions, unknown identity, rotated key and mismatched digest fail closed', async () => {
  assert.equal(await createHybridAgentVerifier({ trustedKeys: trust(), lookupProof: async () => { throw Error('offline'); } })(request(envelope)), false);
  const rotated = trust(); rotated[0].keyEpoch = 2;
  assert.equal(await verifyWith(good, rotated)(request(envelope)), false);
  const unknown = trust(); unknown[0].agentIdentity.id = 'unknown';
  assert.equal(await verifyWith(good, unknown)(request(envelope)), false);
  assert.equal(await verifyWith(good)({ envelope, authorizationCommitment: '0'.repeat(64) }), false);
});
test('operator key snapshot cannot be mutated by caller after construction', async () => {
  const keys = trust();
  const verify = verifyWith(good, keys);
  keys[0].mlDsa65PublicKey.fill(0); keys[0].keyEpoch = 8; keys[0].agentIdentity.id = 'changed';
  assert.equal(await verify(request(envelope)), true);
});
test('hybrid verifier integrates with real file-backed authorization and persistent replay rejection', async t => {
  const root = await mkdtemp(join(tmpdir(), 'qpc-hybrid-store-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const store = new FileIntentStore(root);
  const nowUnix = envelope.time.issuedAtUnix + 1;
  await store.create(envelope, { nowUnix });
  const result = await store.authorize(envelope, { nowUnix, verifyAgentAuthorization: verifyWith(good) });
  assert.equal(result.state, 'AUTHORIZED');
  const reopened = new FileIntentStore(root);
  await assert.rejects(reopened.authorize(envelope, { nowUnix, verifyAgentAuthorization: verifyWith(good) }), /already consumed/);
});
test('operator-pinned store verifier cannot be replaced by per-call authorization', async t => {
  const root = await mkdtemp(join(tmpdir(), 'qpc-hybrid-pinned-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const nowUnix = envelope.time.issuedAtUnix + 1;
  const bad = structuredClone(good); bad.mlDsa65Signature[0] ^= 1;
  const blocked = new FileIntentStore(root, { authorizationVerifier: verifyWith(bad) });
  await blocked.create(envelope, { nowUnix });
  await assert.rejects(blocked.authorize(envelope, { nowUnix, verifyAgentAuthorization: async () => true }), /operator-pinned/);
  await assert.rejects(blocked.authorize(envelope, { nowUnix }), /verification failed/);
  const configured = new FileIntentStore(root, { authorizationVerifier: verifyWith(good) });
  assert.equal((await configured.authorize(envelope, { nowUnix })).state, 'AUTHORIZED');
});
test('valid signatures do not bypass envelope expiry', async () => {
  let consumed = false;
  await assert.rejects(authorizeCrossL1Envelope(envelope, {
    nowUnix: envelope.time.expiresAtUnix, verifyAgentAuthorization: verifyWith(good),
    consumeReplay: async () => { consumed = true; return true; },
  }), /expired/);
  assert.equal(consumed, false);
});
test('private keys are forbidden in verifier trust configuration', () => {
  const keys = trust();
  keys[0].ed25519PublicKey = classical.privateKey.export({ type: 'pkcs8', format: 'pem' });
  assert.throws(() => verifyWith(good, keys), /public key/);
});
test('unresponsive proof lookup is bounded and aborted', async () => {
  let signal;
  const verify = createHybridAgentVerifier({ trustedKeys: trust(), lookupTimeoutMs: 10,
    lookupProof: async (_, options) => { signal = options?.signal; return new Promise(() => {}); } });
  const outcome = await Promise.race([
    verify(request(envelope)), new Promise(resolve => setTimeout(() => resolve('unbounded'), 100)),
  ]);
  assert.equal(outcome, false);
  assert.equal(signal.aborted, true);
});
test('invalid operator configuration rejects eagerly', () => {
  assert.throws(() => verifyWith(good, []), /trusted keys/);
  const duplicate = trust(); duplicate.push(duplicate[0]);
  assert.throws(() => verifyWith(good, duplicate), /duplicate/);
  const wrong = trust(); wrong[0].mlDsa65PublicKey = new Uint8Array(3);
  assert.throws(() => verifyWith(good, wrong), /length/);
});
