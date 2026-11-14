import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, writeFile, rm, mkdir, access } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { FileIntentStore } from '../src/intent-store.mjs';

const fixture = JSON.parse(await readFile(new URL('../../docs/fixtures/cross-l1-v1.json', import.meta.url)));
const NOW = fixture.envelope.time.issuedAtUnix + 1;
const envelope = () => structuredClone(fixture.envelope);
const auth = { nowUnix: NOW, verifyAgentAuthorization: async () => true };
async function setup(t) {
  const root = await mkdtemp(join(tmpdir(), 'qpc-recovery-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  return { root, store: new FileIntentStore(root, { lockTimeoutMs: 0 }) };
}
async function authorize(store, e = envelope()) {
  const record = await store.create(e, { nowUnix: NOW });
  await store.authorize(e, auth);
  return record.id;
}

test('missing or orphan replay indexes fail closed instead of permitting reauthorization', async t => {
  const { root, store } = await setup(t);
  const id = await authorize(store);
  const original = JSON.parse(await readFile(join(root, 'state.json'), 'utf8'));
  for (const mutate of [s => { s.nonces = {}; }, s => { s.intents = {}; }, s => { s.nonces = {}; s.intents = {}; }, s => { s.nonces.orphan = id; }, s => { s.records[id].history[1].state = 'CREATED'; }, s => { s.records[id].envelope.amount.sourceAtomic = '2'; }]) {
    const state = structuredClone(original); mutate(state);
    await writeFile(join(root, 'state.json'), JSON.stringify(state));
    const changed = envelope(); changed.amount.sourceAtomic = '2';
    await assert.rejects(store.create(changed, { nowUnix: NOW }), /invalid persistent/);
    await assert.rejects(store.get(id), /invalid persistent/);
  }
});

test('legacy V1 replay indexes fail closed under V2 instead of being silently discarded', async t => {
  const { root, store } = await setup(t);
  const id = await authorize(store);
  const state = JSON.parse(await readFile(join(root, 'state.json'), 'utf8'));
  const e = state.records[id].envelope;
  const legacyDomain = createHash('sha256')
    .update('QPC-CROSS-L1-REPLAY-V1\0')
    .update(JSON.stringify([
      e.replayDomain.authorityId,
      e.replayDomain.scope,
      e.source.networkId,
      e.source.nativeChainId,
      e.source.accountId,
      e.agentIdentity.namespace,
      e.agentIdentity.id,
    ]))
    .digest('hex');
  state.nonces = { [JSON.stringify([legacyDomain, e.nonce])]: id };
  state.intents = { [JSON.stringify([legacyDomain, e.intentId])]: id };
  const legacyBytes = JSON.stringify(state);
  await writeFile(join(root, 'state.json'), legacyBytes);

  await assert.rejects(store.get(id), /invalid persistent store: replay index mismatch/);
  await assert.rejects(store.authorize(e, auth), /invalid persistent store: replay index mismatch/);
  assert.equal(await readFile(join(root, 'state.json'), 'utf8'), legacyBytes);
});

test('restart recovery atomically quarantines uncertain submissions without releasing replay', async t => {
  const { root, store } = await setup(t);
  const pending = envelope();
  const submitted = envelope(); submitted.nonce = '200'; submitted.intentId = 'submitted';
  const canceled = envelope(); canceled.nonce = '201'; canceled.intentId = 'canceled';
  const a = await authorize(store, pending), b = await authorize(store, submitted), c = await authorize(store, canceled);
  await store.reserveSubmission(a, { nowUnix: NOW + 1 });
  await store.recordSubmission(b, { nowUnix: NOW + 1, transactionId: 'test-only-tx' });
  await store.cancel(c, { nowUnix: NOW + 1, reason: 'policy-revoked' });
  const before = JSON.parse(await readFile(join(root, 'state.json'), 'utf8'));
  const restarted = new FileIntentStore(root);
  const recovered = await restarted.recover({ nowUnix: NOW + 2 });
  assert.deepEqual(new Set(recovered.map(r => r.id)), new Set([a, b]));
  assert(recovered.every(r => r.state === 'RECONCILING'));
  assert.equal((await restarted.get(b)).transactionId, 'test-only-tx');
  assert.equal((await restarted.get(c)).state, 'CANCELED');
  assert.deepEqual(await restarted.recover({ nowUnix: NOW + 3 }), []);
  const after = JSON.parse(await readFile(join(root, 'state.json'), 'utf8'));
  assert.deepEqual(after.nonces, before.nonces); assert.deepEqual(after.intents, before.intents);
  for (const [id, e] of [[a, pending], [b, submitted], [c, canceled]]) {
    await assert.rejects(restarted.authorize(e, auth), /already consumed/);
    await assert.rejects(restarted.reserveSubmission(id, { nowUnix: NOW + 4 }), /transition/);
    await assert.rejects(restarted.settle(id), /NOT_IMPLEMENTED/);
    await assert.rejects(restarted.settle(id), /NOT_IMPLEMENTED/);
  }
});

test('policy revocation during authorization cannot overwrite cancellation or consume replay', async t => {
  const { root, store } = await setup(t);
  const e = envelope(), { id } = await store.create(e, { nowUnix: NOW });
  let resume, entered;
  const started = new Promise(resolve => { entered = resolve; });
  const approval = new Promise(resolve => { resume = resolve; });
  const pending = store.authorize(e, { nowUnix: NOW + 1, verifyAgentAuthorization: async () => { entered(); return approval; } });
  await started;
  await new FileIntentStore(root).cancel(id, { nowUnix: NOW + 1, reason: 'policy-revoked' });
  resume(true);
  await assert.rejects(pending, /transition/);
  const state = JSON.parse(await readFile(join(root, 'state.json'), 'utf8'));
  assert.equal(state.records[id].state, 'CANCELED');
  assert.deepEqual(state.nonces, {}); assert.deepEqual(state.intents, {});
  const rejected = envelope(); rejected.intentId = 'revoked'; rejected.nonce = '202';
  const r = await store.create(rejected, { nowUnix: NOW + 1 });
  await assert.rejects(store.authorize(rejected, { nowUnix: NOW + 1, verifyAgentAuthorization: async () => false }), /verification failed/);
  assert.equal((await store.get(r.id)).state, 'CREATED');
});

// Instrument only the child process's filesystem APIs: no production failure hook.
// SIGKILL before rename must preserve the old file; after rename must retain both
// replay indexes and AUTHORIZED together. Neither crash may steal the dead lock.
for (const stage of ['before-rename', 'after-rename']) {
  test(`actual process crash ${stage} never persists partial authorization`, { timeout: 10000 }, async t => {
    const { root, store } = await setup(t);
    const e = envelope(), { id } = await store.create(e, { nowUnix: NOW });
    const script = `import fs from 'node:fs/promises';
      const rename=fs.rename; fs.rename=async(...args)=>{
        if(${JSON.stringify(stage)}==='after-rename') await rename(...args);
        process.stdout.write('CRASH-POINT\\n'); await new Promise(resolve=>setTimeout(resolve,60000));
      };
      const {syncBuiltinESMExports}=await import('node:module');syncBuiltinESMExports();
      const {FileIntentStore}=await import(${JSON.stringify(new URL('../src/intent-store.mjs', import.meta.url).href)});
      const s=new FileIntentStore(process.argv[1]);const r=await s.get(process.argv[2]);
      await s.authorize(r.envelope,{nowUnix:${NOW},verifyAgentAuthorization:async()=>true});`;
    const child = spawn(process.execPath, ['--input-type=module', '-e', script, root, id], { stdio: ['ignore', 'pipe', 'pipe'] });
    t.after(() => { if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL'); });
    const output = await Promise.race([
      once(child.stdout, 'data').then(([data]) => data.toString()),
      once(child, 'exit').then(([code]) => { throw new Error(`child exited early: ${code}`); }),
    ]);
    assert.match(output, /CRASH-POINT/);
    const exited = once(child, 'exit'); child.kill('SIGKILL'); await exited;
    await access(join(root, '.lock'));
    const persisted = JSON.parse(await readFile(join(root, 'state.json'), 'utf8'));
    assert.equal(persisted.records[id].state, stage === 'before-rename' ? 'CREATED' : 'AUTHORIZED');
    assert.equal(Object.keys(persisted.nonces).length, stage === 'before-rename' ? 0 : 1);
    assert.equal(Object.keys(persisted.intents).length, stage === 'before-rename' ? 0 : 1);
    await assert.rejects(store.recover({ nowUnix: NOW + 1 }), /locked/);
    // Test harness has joined the killed process, so no live writer can own lock.
    await rm(join(root, '.lock'), { recursive: true });
    assert.deepEqual(await store.recover({ nowUnix: NOW + 1 }), []);
    if (stage === 'before-rename') assert.equal((await store.authorize(e, auth)).state, 'AUTHORIZED');
    else await assert.rejects(store.authorize(e, auth), /already consumed/);
  });
}

test('failed and forged evidence after recovery cannot confirm or settle, even twice', async t => {
  const { root, store } = await setup(t);
  const id = await authorize(store);
  await store.recordSubmission(id, { nowUnix: NOW + 1, transactionId: 'test-only-tx' });
  const restarted = new FileIntentStore(root);
  await restarted.recover({ nowUnix: NOW + 2 });
  const e = fixture.envelope;
  const evidence = {
    schema: 'qpc.cross-l1.settlement-evidence', version: 1, authorizationCommitment: id,
    evidenceClass: e.settlement.evidenceClass, destinationNetworkId: e.destination.networkId,
    destinationAdapterId: e.destination.adapterId, destinationAssetId: e.asset.destinationAssetId,
    amountAtomic: e.amount.destinationAtomic, recipientCommitment: e.destination.recipientCommitment,
    transactionId: 'test-only-tx', inclusionReference: 'fabricated-block', confirmations: 999,
    finalityStatus: 'FINALIZED', proofDigest: 'e'.repeat(64), observedAtUnix: NOW + 2,
  };
  const before = await restarted.get(id);
  for (const accepted of [false, true]) {
    await assert.rejects(restarted.confirm(id, { nowUnix: NOW + 3, evidence, verifyDestinationEvidence: async () => accepted }), /verification failed|NOT_VERIFIED/);
    await assert.rejects(restarted.settle(id, { nowUnix: NOW + 3 }), /NOT_IMPLEMENTED/);
    assert.deepEqual(await restarted.get(id), before);
  }
});

test('recovery is all-or-nothing if any record has a later durable timestamp', async t => {
  const { root, store } = await setup(t);
  const e = envelope(); e.nonce = '300'; e.intentId = 'later';
  const a = await authorize(store), b = await authorize(store, e);
  await store.reserveSubmission(a, { nowUnix: NOW + 1 });
  await store.reserveSubmission(b, { nowUnix: NOW + 5 });
  const before = await readFile(join(root, 'state.json'), 'utf8');
  await assert.rejects(store.recover({ nowUnix: NOW + 2 }), /time moved backwards/);
  assert.equal(await readFile(join(root, 'state.json'), 'utf8'), before);
  assert.equal((await store.recover({ nowUnix: NOW + 6 })).length, 2);
});

test('legacy confirmations are quarantined on recovery, never grandfathered into settlement', async t => {
  const { root, store } = await setup(t);
  const id = await authorize(store);
  await store.recordSubmission(id, { nowUnix: NOW + 1, transactionId: 'old-untrusted-tx' });
  const state = JSON.parse(await readFile(join(root, 'state.json'), 'utf8'));
  const r = state.records[id];
  for (const next of ['CONFIRMED', 'SETTLED']) {
    r.state = next; r.revision++; r.history.push({ state: next, atUnix: NOW + 1 });
  }
  r.confirmation = { state: 'EVIDENCE_VERIFIED' };
  await writeFile(join(root, 'state.json'), JSON.stringify(state));
  const [recovered] = await store.recover({ nowUnix: NOW + 2 });
  assert.equal(recovered.state, 'RECONCILING');
  assert.equal(recovered.transactionId, 'old-untrusted-tx');
  assert.equal(Object.hasOwn(recovered, 'confirmation'), false);
  await assert.rejects(store.settle(id), /NOT_IMPLEMENTED/);
  await assert.rejects(store.authorize(envelope(), auth), /already consumed/);
});

test('orphan temporary files are never treated as committed authorization', async t => {
  const { root, store } = await setup(t);
  const { id } = await store.create(envelope(), { nowUnix: NOW });
  await writeFile(join(root, '.state-orphan.tmp'), '{malformed');
  assert.deepEqual(await store.recover({ nowUnix: NOW + 1 }), []);
  assert.equal((await store.get(id)).state, 'CREATED');
  await mkdir(join(root, '.lock'));
  await assert.rejects(store.recover({ nowUnix: NOW + 1 }), /locked/);
});
