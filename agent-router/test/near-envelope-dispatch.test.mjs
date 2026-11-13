import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { buildNearEnvelopeAuthorizationDispatch } from '../src/index.mjs';
const fixture = JSON.parse(readFileSync(new URL('../../docs/fixtures/cross-l1-v1.json', import.meta.url)));
test('full envelope client prepares the actual NEAR authorization method without a signer', () => {
  const result = buildNearEnvelopeAuthorizationDispatch({ envelope: fixture.envelope, nowUnix: fixture.envelope.time.notBeforeUnix });
  assert.equal(result.state, 'PREPARED_AUTHORIZATION_ONLY');
  assert.equal(result.qpcCall.receiverId, 'qpc.testnet');
  assert.equal(result.qpcCall.methodName, 'authorize_envelope');
  assert.equal(result.qpcCall.args.authorization_commitment, fixture.authorizationCommitment);
  assert.deepEqual(result.qpcCall.args.envelope, fixture.envelope);
  assert.equal(result.qpcCall.deposit, '0');
  assert.ok(Object.isFrozen(result.qpcCall.args.envelope));
  assert.equal(result.execution, false);
});
test('full envelope dispatch rejects mismatched agent and invalid contract receiver', () => {
  const bad = structuredClone(fixture.envelope); bad.source.accountId = 'other.testnet';
  assert.throws(() => buildNearEnvelopeAuthorizationDispatch({ envelope: bad, nowUnix: bad.time.notBeforeUnix }), /agent/);
  const invalid = structuredClone(fixture.envelope); invalid.source.adapterId = 'near:contract';
  assert.throws(() => buildNearEnvelopeAuthorizationDispatch({ envelope: invalid, nowUnix: invalid.time.notBeforeUnix }), /contract/);
});
test('full envelope dispatch rejects invalid native identity and expiry', () => {
  const bad = structuredClone(fixture.envelope); bad.source.nativeChainId = 'mainnet';
  assert.throws(() => buildNearEnvelopeAuthorizationDispatch({ envelope: bad, nowUnix: bad.time.notBeforeUnix }), /network/);
  assert.throws(() => buildNearEnvelopeAuthorizationDispatch({ envelope: fixture.envelope, nowUnix: fixture.envelope.time.expiresAtUnix }), /window/);
});
