import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile, writeFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FileIntentStore } from '../src/intent-store.mjs';
import { computeCrossL1AuthorizationCommitment, verifyCrossL1SettlementEvidence } from '../src/cross-l1-envelope.mjs';
const { envelope } = JSON.parse(await readFile(new URL('../../docs/fixtures/cross-l1-v1.json', import.meta.url)));
const nowUnix = envelope.time.issuedAtUnix + 1;
const evidence = {
  schema: 'qpc.cross-l1.settlement-evidence', version: 1,
  authorizationCommitment: computeCrossL1AuthorizationCommitment(envelope),
  evidenceClass: envelope.settlement.evidenceClass,
  destinationNetworkId: envelope.destination.networkId,
  destinationAdapterId: envelope.destination.adapterId,
  destinationAssetId: envelope.asset.destinationAssetId,
  amountAtomic: envelope.amount.destinationAtomic,
  recipientCommitment: envelope.destination.recipientCommitment,
  transactionId: 'fabricated-tx', inclusionReference: 'fabricated-block',
  confirmations: 999, finalityStatus: 'FINALIZED', proofDigest: 'e'.repeat(64), observedAtUnix: nowUnix,
};
test('caller boolean cannot promote fabricated evidence to verified settlement', async () => {
  const result = await verifyCrossL1SettlementEvidence({ authorization: envelope, evidence, verifyDestinationEvidence: async () => true });
  assert.equal(result.state, 'NOT_VERIFIED');
});
test('caller boolean cannot confirm or settle a durable intent', async t => {
  const root = await mkdtemp(join(tmpdir(), 'qpc-forged-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const store = new FileIntentStore(root);
  const { id } = await store.create(envelope, { nowUnix });
  await store.authorize(envelope, { nowUnix, verifyAgentAuthorization: async () => true });
  await store.recordSubmission(id, { nowUnix, transactionId: evidence.transactionId });
  await assert.rejects(store.confirm(id, { nowUnix, evidence, verifyDestinationEvidence: async () => true }), /NOT_VERIFIED/);
  assert.equal((await store.get(id)).state, 'SUBMITTED');
  await assert.rejects(store.settle(id, { nowUnix }), /NOT_IMPLEMENTED/);
  // Old versions persisted CONFIRMED records from callback-only evidence. They
  // must not be grandfathered into settlement after upgrading.
  const file = join(root, 'state.json');
  const legacy = JSON.parse(await readFile(file, 'utf8'));
  legacy.records[id].state = 'CONFIRMED';
  legacy.records[id].confirmation = { state: 'EVIDENCE_VERIFIED', transactionId: evidence.transactionId };
  await writeFile(file, JSON.stringify(legacy));
  await assert.rejects(new FileIntentStore(root).settle(id, { nowUnix }), /NOT_IMPLEMENTED/);
  assert.equal((await store.get(id)).state, 'CONFIRMED');
});
