import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { FileIntentStore } from "../src/intent-store.mjs";
import { computeCrossL1AuthorizationCommitment } from "../src/cross-l1-envelope.mjs";

const fixture = JSON.parse(await readFile(new URL("../../docs/fixtures/cross-l1-v1.json", import.meta.url)));
const NOW = fixture.envelope.time.issuedAtUnix + 1;
const envelope = () => structuredClone(fixture.envelope);
async function setup(t) {
  const root = await mkdtemp(join(tmpdir(), "qpc-intents-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  return { root, store: new FileIntentStore(root) };
}
const auth = { nowUnix: NOW, verifyAgentAuthorization: async () => true };
function evidence(e = fixture.envelope) {
  return {
    schema: "qpc.cross-l1.settlement-evidence", version: 1,
    authorizationCommitment: computeCrossL1AuthorizationCommitment(e),
    evidenceClass: e.settlement.evidenceClass,
    destinationNetworkId: e.destination.networkId, destinationAdapterId: e.destination.adapterId,
    destinationAssetId: e.asset.destinationAssetId, amountAtomic: e.amount.destinationAtomic,
    recipientCommitment: e.destination.recipientCommitment, transactionId: "tx-1",
    inclusionReference: "height-42", confirmations: 12, finalityStatus: "FINALIZED",
    proofDigest: "e".repeat(64), observedAtUnix: NOW + 5,
  };
}

test("atomic authorization races allow one winner and survive fresh-process restart", async t => {
  const { root, store } = await setup(t);
  const created = await store.create(envelope(), { nowUnix: NOW });
  const calls = await Promise.allSettled(Array.from({ length: 12 }, () =>
    new FileIntentStore(root).authorize(envelope(), auth)));
  assert.equal(calls.filter(c => c.status === "fulfilled").length, 1);
  assert.equal((await new FileIntentStore(root).get(created.id)).state, "AUTHORIZED");
  const script = `import {FileIntentStore} from ${JSON.stringify(new URL('../src/intent-store.mjs',import.meta.url).href)};
    const s=new FileIntentStore(process.argv[1]);
    const r=await s.get(process.argv[2]); console.log(r.state);
    try {await s.authorize(r.envelope,{nowUnix:${NOW},verifyAgentAuthorization:async()=>true});process.exit(2)}catch{}`;
  const { stdout } = await promisify(execFile)(process.execPath, ["--input-type=module", "-e", script, root, created.id]);
  assert.equal(stdout.trim(), "AUTHORIZED");
});

test("separate OS processes race on one durable replay transaction", async t => {
  const { root, store } = await setup(t);
  const { id } = await store.create(envelope(), { nowUnix: NOW });
  const script = `import {FileIntentStore} from ${JSON.stringify(new URL('../src/intent-store.mjs',import.meta.url).href)};
    const s=new FileIntentStore(process.argv[1]);const r=await s.get(process.argv[2]);
    try{await s.authorize(r.envelope,{nowUnix:${NOW},verifyAgentAuthorization:async()=>true});console.log('winner')}
    catch(e){if(!/already consumed/.test(e.message))throw e;console.log('replay')}`;
  const results = await Promise.all(Array.from({ length: 6 }, () =>
    promisify(execFile)(process.execPath, ["--input-type=module", "-e", script, root, id])));
  assert.equal(results.filter(r => r.stdout.trim() === "winner").length, 1);
  assert.equal(results.filter(r => r.stdout.trim() === "replay").length, 5);
});

test("in-flight reservation is atomic and timeout uncertainty cannot permit a second send", async t => {
  const { store } = await setup(t);
  const { id } = await store.create(envelope(), { nowUnix: NOW });
  await store.authorize(envelope(), auth);
  const races = await Promise.allSettled([store.reserveSubmission(id, { nowUnix: NOW + 1 }), store.reserveSubmission(id, { nowUnix: NOW + 1 })]);
  assert.equal(races.filter(r => r.status === "fulfilled").length, 1);
  assert.equal((await store.fail(id, { nowUnix: NOW + 2, reason: "broadcast-timeout" })).state, "RECONCILING");
  await assert.rejects(store.reserveSubmission(id, { nowUnix: NOW + 3 }), /transition/);
  await assert.rejects(store.cancel(id, { nowUnix: NOW + 3, reason: "unsafe" }), /transition/);
  await store.confirm(id, { nowUnix: NOW + 6, evidence: evidence(), verifyDestinationEvidence: async () => true });
  assert.equal((await store.settle(id, { nowUnix: NOW + 7 })).state, "SETTLED");
});

test("nonce and intent uniqueness are independent, scoped, and never released on cancel", async t => {
  const { store } = await setup(t);
  await store.create(envelope(), { nowUnix: NOW });
  await store.authorize(envelope(), auth);
  for (const changed of [{ intentId: "different-intent" }, { nonce: "8" }]) {
    const e = Object.assign(envelope(), changed);
    await store.create(e, { nowUnix: NOW });
    await assert.rejects(store.authorize(e, auth), /already consumed/);
  }
  const id = computeCrossL1AuthorizationCommitment(envelope());
  await store.cancel(id, { nowUnix: NOW + 1, reason: "owner-request" });
  await assert.rejects(store.authorize(envelope(), auth), /already consumed/);
  const otherDomain = envelope();
  otherDomain.source.nativeChainId = "mainnet";
  await store.create(otherDomain, { nowUnix: NOW });
  assert.equal((await store.authorize(otherDomain, auth)).state, "AUTHORIZED");
});

test("verifier failure does not consume replay or move CREATED", async t => {
  const { store } = await setup(t);
  const created = await store.create(envelope(), { nowUnix: NOW });
  await assert.rejects(store.authorize(envelope(), { ...auth, verifyAgentAuthorization: async () => false }), /verification failed/);
  assert.equal((await store.get(created.id)).state, "CREATED");
  assert.equal((await store.authorize(envelope(), auth)).state, "AUTHORIZED");
});

test("state machine requires verified matching finality, forbids duplicate submission and stale updates", async t => {
  const { store } = await setup(t);
  const { id } = await store.create(envelope(), { nowUnix: NOW });
  await assert.rejects(store.recordSubmission(id, { nowUnix: NOW, transactionId: "tx-1" }), /transition/);
  await store.authorize(envelope(), auth);
  await store.recordSubmission(id, { nowUnix: NOW + 1, transactionId: "tx-1", expectedRevision: 1 });
  await assert.rejects(store.recordSubmission(id, { nowUnix: NOW + 2, transactionId: "tx-2" }), /transition/);
  await assert.rejects(store.settle(id, { nowUnix: NOW + 2 }), /transition/);
  await assert.rejects(store.cancel(id, { nowUnix: NOW + 2, reason: "cancel" }), /transition/);
  await assert.rejects(store.confirm(id, { nowUnix: NOW + 2, evidence: evidence(), verifyDestinationEvidence: async () => false }), /verification failed/);
  await assert.rejects(store.confirm(id, { nowUnix: NOW + 2, evidence: { ...evidence(), transactionId: "other" }, verifyDestinationEvidence: async () => true }), /transaction/);
  await store.confirm(id, { nowUnix: NOW + 6, evidence: evidence(), verifyDestinationEvidence: async () => true });
  await assert.rejects(store.settle(id, { nowUnix: NOW + 7, expectedRevision: 2 }), /revision/);
  assert.equal((await store.settle(id, { nowUnix: NOW + 7 })).state, "SETTLED");
  assert.equal((await store.reconcile(id, { nowUnix: NOW + 8, reason: "reorg" })).state, "RECONCILING");
  await store.confirm(id, { nowUnix: NOW + 9, evidence: evidence(), verifyDestinationEvidence: async () => true });
  assert.equal((await store.settle(id, { nowUnix: NOW + 10 })).state, "SETTLED");
});

test("failures, timeout uncertainty, cancellation and expiry fail closed", async t => {
  const { store } = await setup(t);
  const e = envelope();
  const { id } = await store.create(e, { nowUnix: NOW });
  await assert.rejects(store.expire(id, { nowUnix: NOW }), /not expired/);
  await store.authorize(e, auth);
  await store.recordSubmission(id, { nowUnix: NOW + 1, transactionId: "tx-1" });
  assert.equal((await store.fail(id, { nowUnix: NOW + 2, reason: "rpc-timeout" })).state, "RECONCILING");
  await assert.rejects(store.expire(id, { nowUnix: e.time.expiresAtUnix }), /transition/);
  const e2 = envelope(); e2.intentId = "new"; e2.nonce = "8";
  const r2 = await store.create(e2, { nowUnix: NOW });
  assert.equal((await store.expire(r2.id, { nowUnix: e.time.expiresAtUnix })).state, "EXPIRED");
  const e3 = envelope(); e3.intentId = "third"; e3.nonce = "9";
  const r3 = await store.create(e3, { nowUnix: NOW });
  assert.equal((await store.fail(r3.id, { nowUnix: NOW, reason: "validation" })).state, "FAILED");
});

test("corruption and existing locks fail closed, never reset replay state", async t => {
  const { store, root } = await setup(t);
  await store.create(envelope(), { nowUnix: NOW });
  await writeFile(join(root, "state.json"), "{broken");
  await assert.rejects(store.get(fixture.authorizationCommitment));
  await assert.rejects(store.create(envelope(), { nowUnix: NOW }));
  await mkdir(join(root, ".lock"));
  const locked = new FileIntentStore(root, { lockTimeoutMs: 20 });
  await assert.rejects(locked.create(envelope(), { nowUnix: NOW }), /locked/);
});

test("snapshots cannot be mutated and late authorization callbacks cannot change persisted envelope", async t => {
  const { store } = await setup(t);
  const e = envelope();
  const created = await store.create(e, { nowUnix: NOW });
  assert.throws(() => { created.envelope.amount.sourceAtomic = "1"; }, TypeError);
  await store.authorize(e, { nowUnix: NOW, verifyAgentAuthorization: async ({ envelope: frozen }) => {
    e.amount.sourceAtomic = "1";
    assert.throws(() => { frozen.amount.sourceAtomic = "1"; }, TypeError);
    return true;
  } });
  assert.equal((await store.get(created.id)).envelope.amount.sourceAtomic, fixture.envelope.amount.sourceAtomic);
});
