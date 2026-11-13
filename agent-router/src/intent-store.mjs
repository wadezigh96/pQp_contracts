import { mkdir, open, readFile, rename, rmdir, unlink } from "node:fs/promises";
import { join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import {
  canonicalizeCrossL1AuthorizationEnvelope,
  computeCrossL1AuthorizationCommitment,
  computeCrossL1ReplayDomainKey,
  authorizeCrossL1Envelope,
  verifyCrossL1SettlementEvidence,
} from "./cross-l1-envelope.mjs";

const ID = /^[0-9a-f]{64}$/;
const TEXT = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,255}$/;
function check(value, message) { if (!value) throw new Error(message); }
function time(value) {
  check(Number.isSafeInteger(value) && value >= 0 && !Object.is(value, -0), "invalid nowUnix");
  return value;
}
function text(value, label) { check(typeof value === "string" && TEXT.test(value), `invalid ${label}`); return value; }
function immutable(value) {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) immutable(child);
    Object.freeze(value);
  }
  return value;
}
function copy(value) { return immutable(structuredClone(value)); }

/**
 * LOCAL PROTOTYPE ONLY. One private directory on a local filesystem supporting
 * atomic mkdir/rename and fsync; not NFS, multi-host consensus, or an RPC API.
 * The caller is a trusted orchestrator: it must authenticate cancellation,
 * administrative failures and submission reports. No signing/broadcast occurs.
 * Mandatory injected authorization/evidence verifiers must be real integrations
 * in production; tests use explicit mocks, not a production verifier.
 *
 * A lock covers read/check/write of BOTH replay indexes and lifecycle record.
 * Replacement writes fsync file, rename, fsync directory before success. A crash
 * may leave .lock: FAIL CLOSED; operator must prove no writer is alive before
 * removing it. Never automatically steal locks. Storage rollback/deletion,
 * malicious local writers, hardware fsync lies, and lost-disk recovery are not
 * defended; production needs transactional replicated durable storage/backups.
 * The store retains full envelopes/evidence summaries (sensitive metadata) and
 * permanently retains replay keys. Use OS access controls/encryption externally.
 */
export class FileIntentStore {
  #root;
  #lockTimeoutMs;
  constructor(root, { lockTimeoutMs = 5000 } = {}) {
    check(typeof root === "string" && root.length > 0, "store directory is required");
    check(Number.isSafeInteger(lockTimeoutMs) && lockTimeoutMs >= 0, "invalid lock timeout");
    this.#root = resolve(root);
    this.#lockTimeoutMs = lockTimeoutMs;
  }

  async #read() {
    let raw;
    try { raw = await readFile(join(this.#root, "state.json"), "utf8"); }
    catch (error) {
      if (error.code === "ENOENT") return { version: 1, records: {}, nonces: {}, intents: {} };
      throw error;
    }
    const state = JSON.parse(raw); // corruption propagates; never silently reset
    check(state.version === 1 && [state.records, state.nonces, state.intents].every(
      x => x !== null && typeof x === "object" && !Array.isArray(x)), "invalid persistent store");
    return state;
  }

  async #write(state) {
    const temporary = join(this.#root, `.state-${randomUUID()}.tmp`);
    let handle;
    try {
      handle = await open(temporary, "wx", 0o600);
      await handle.writeFile(JSON.stringify(state));
      await handle.sync();
      await handle.close(); handle = undefined;
      await rename(temporary, join(this.#root, "state.json"));
      const directory = await open(this.#root, "r");
      try { await directory.sync(); } finally { await directory.close(); }
    } finally {
      if (handle) await handle.close();
      await unlink(temporary).catch(error => { if (error.code !== "ENOENT") throw error; });
    }
  }

  async #transaction(update) {
    await mkdir(this.#root, { recursive: true, mode: 0o700 });
    const lock = join(this.#root, ".lock");
    const deadline = Date.now() + this.#lockTimeoutMs;
    for (;;) {
      try { await mkdir(lock, { mode: 0o700 }); break; }
      catch (error) {
        if (error.code !== "EEXIST") throw error;
        check(Date.now() < deadline, "intent store locked; manual crash recovery may be required");
        await new Promise(resolve => setTimeout(resolve, 5));
      }
    }
    try {
      const state = await this.#read();
      const result = update(state); // deliberately synchronous: no callback under lock
      await this.#write(state);
      return copy(result);
    } finally { await rmdir(lock); }
  }

  async get(id) {
    check(ID.test(id), "invalid intent record id");
    const state = await this.#read();
    check(Object.hasOwn(state.records, id), "intent not found");
    return copy(state.records[id]);
  }

  async create(input, { nowUnix } = {}) {
    const envelope = canonicalizeCrossL1AuthorizationEnvelope(input);
    const id = computeCrossL1AuthorizationCommitment(envelope);
    const now = time(nowUnix);
    return this.#transaction(state => {
      check(!Object.hasOwn(state.records, id), "intent already exists");
      const record = {
        id, envelope, state: "CREATED", revision: 0,
        createdAtUnix: now, updatedAtUnix: now,
        history: [{ state: "CREATED", atUnix: now }],
      };
      state.records[id] = record;
      return record;
    });
  }

  async authorize(input, options = {}) {
    // authorizeCrossL1Envelope snapshots before invoking external verifier.
    let authorized;
    const captured = { nowUnix: options.nowUnix, expectedRevision: options.expectedRevision };
    const verifier = options.verifyAgentAuthorization;
    await authorizeCrossL1Envelope(input, {
      nowUnix: captured.nowUnix,
      verifyAgentAuthorization: verifier,
      consumeReplay: async ({ replayDomainKey, nonce, intentId, authorizationCommitment }) => {
        let consumed = false;
        await this.#transaction(state => {
          const nonceKey = JSON.stringify([replayDomainKey, nonce]);
          const intentKey = JSON.stringify([replayDomainKey, intentId]);
          if (Object.hasOwn(state.nonces, nonceKey) || Object.hasOwn(state.intents, intentKey)) return null;
          const record = state.records[authorizationCommitment];
          check(record, "create the intent before authorization");
          check(computeCrossL1ReplayDomainKey(record.envelope) === replayDomainKey, "replay domain mismatch");
          this.#advance(record, "AUTHORIZED", ["CREATED"], captured);
          state.nonces[nonceKey] = authorizationCommitment;
          state.intents[intentKey] = authorizationCommitment;
          authorized = copy(record);
          consumed = true;
          return record;
        });
        return consumed;
      },
    });
    return authorized;
  }

  #advance(record, next, allowed, { nowUnix, expectedRevision, reason } = {}) {
    const now = time(nowUnix);
    if (expectedRevision !== undefined) check(record.revision === expectedRevision, "stale intent revision");
    check(allowed.includes(record.state), `invalid transition ${record.state} -> ${next}`);
    check(now >= record.updatedAtUnix, "time moved backwards");
    if (["AUTHORIZED", "SUBMITTING", "SUBMITTED"].includes(next)) {
      check(now >= record.envelope.time.notBeforeUnix && now < record.envelope.time.expiresAtUnix,
        "authorization is outside valid time window");
    }
    record.state = next;
    record.revision += 1;
    record.updatedAtUnix = now;
    record.history.push({ state: next, atUnix: now, ...(reason ? { reason: text(reason, "reason") } : {}) });
    return record;
  }

  async #update(id, next, allowed, options, amend = () => {}) {
    check(ID.test(id), "invalid intent record id");
    // Capture primitive options before waiting for the lock.
    const captured = { nowUnix: options?.nowUnix, expectedRevision: options?.expectedRevision, reason: options?.reason };
    return this.#transaction(state => {
      const record = state.records[id];
      check(record, "intent not found");
      this.#advance(record, next, allowed, captured);
      amend(record);
      return record;
    });
  }

  // Reserve BEFORE calling any external sender. A crash/timeout after this write
  // is ambiguous and must reconcile; reservation must never be retried blindly.
  async reserveSubmission(id, options = {}) {
    return this.#update(id, "SUBMITTING", ["AUTHORIZED"], options);
  }

  async recordSubmission(id, options = {}) {
    const transactionId = text(options.transactionId, "transactionId");
    return this.#update(id, "SUBMITTED", ["AUTHORIZED", "SUBMITTING"], options, record => { record.transactionId = transactionId; });
  }

  async confirm(id, options = {}) {
    const prior = await this.get(id);
    const nowUnix = options.nowUnix;
    const expectedRevision = options.expectedRevision ?? prior.revision;
    // Verifier runs outside the lock; CAS prevents a concurrent state change
    // from being overwritten by a stale asynchronous proof verification.
    const verified = await verifyCrossL1SettlementEvidence({
      authorization: prior.envelope,
      evidence: options.evidence,
      verifyDestinationEvidence: options.verifyDestinationEvidence,
    });
    check(verified.transactionId === prior.transactionId ||
      (prior.state === "RECONCILING" && prior.transactionId === undefined), "settlement transaction mismatch");
    return this.#update(id, "CONFIRMED", ["SUBMITTED", "RECONCILING"], { nowUnix, expectedRevision }, record => {
      record.transactionId = verified.transactionId;
      record.confirmation = verified;
    });
  }

  async settle(id, options) {
    return this.#update(id, "SETTLED", ["CONFIRMED"], options, record => {
      check(record.confirmation?.transactionId === record.transactionId, "verified confirmation required");
    });
  }

  async cancel(id, options = {}) {
    text(options.reason, "reason");
    return this.#update(id, "CANCELED", ["CREATED", "AUTHORIZED"], options);
  }

  async expire(id, options = {}) {
    const nowUnix = options.nowUnix;
    return this.#update(id, "EXPIRED", ["CREATED", "AUTHORIZED"], options, record => {
      check(nowUnix >= record.envelope.time.expiresAtUnix, "intent not expired");
    });
  }

  async reconcile(id, options = {}) {
    text(options.reason, "reason");
    return this.#update(id, "RECONCILING", ["SUBMITTING", "SUBMITTED", "CONFIRMED", "SETTLED", "RECONCILING"], options, record => {
      delete record.confirmation;
    });
  }

  async fail(id, options = {}) {
    text(options.reason, "reason");
    const record = await this.get(id);
    const captured = { ...options, expectedRevision: options.expectedRevision ?? record.revision };
    if (["SUBMITTING", "SUBMITTED", "CONFIRMED", "SETTLED", "RECONCILING"].includes(record.state)) return this.reconcile(id, captured);
    return this.#update(id, "FAILED", ["CREATED", "AUTHORIZED"], captured);
  }
}
