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
import { readVerifiedNativeSettlement } from "./testnet-adapters.mjs";

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
const TRUST_FIELDS = ["chain", "networkId", "nativeChainId", "contractAddress", "codeHash", "owner"];
function trustSnapshot(input) {
  check(Array.isArray(input), "nativeTrust must be an array");
  return immutable(input.map(identity => {
    check(identity && [Object.prototype, null].includes(Object.getPrototypeOf(identity)), "plain native trust identity required");
    const descriptors = Object.getOwnPropertyDescriptors(identity);
    check(Reflect.ownKeys(identity).length === TRUST_FIELDS.length && TRUST_FIELDS.every(key =>
      descriptors[key]?.enumerable && "value" in descriptors[key] && typeof descriptors[key].value === "string" && descriptors[key].value.length > 0), "exact native trust data fields required");
    return Object.fromEntries(TRUST_FIELDS.map(key => [key, descriptors[key].value]));
  }));
}

const TRANSITIONS = Object.freeze({
  CREATED: ["AUTHORIZED", "CANCELED", "EXPIRED", "FAILED"],
  AUTHORIZED: ["SUBMITTING", "SUBMITTED", "CANCELED", "EXPIRED", "FAILED"],
  SUBMITTING: ["SUBMITTED", "RECONCILING"],
  SUBMITTED: ["CONFIRMED", "RECONCILING"],
  CONFIRMED: ["SETTLED", "RECONCILING"],
  SETTLED: ["RECONCILING"],
  RECONCILING: ["CONFIRMED", "RECONCILING"],
  CANCELED: [], EXPIRED: [], FAILED: [],
});
function persistentState(state) {
  // Detect partial/corrupt state rather than trusting an independently missing
  // replay map. This is consistency checking, not tamper-proof local storage.
  const invalid = message => check(false, `invalid persistent store: ${message}`);
  const object = x => x !== null && typeof x === "object" && !Array.isArray(x);
  if (!object(state) || state.version !== 1 ||
      ![state.records, state.nonces, state.intents].every(object)) invalid("schema");
  const nonces = Object.create(null), intents = Object.create(null);
  for (const [id, record] of Object.entries(state.records)) {
    if (!ID.test(id) || !object(record) || record.id !== id ||
        !Number.isSafeInteger(record.revision) || record.revision < 0 ||
        !Array.isArray(record.history) || record.history.length !== record.revision + 1) invalid("record");
    let commitment;
    try { commitment = computeCrossL1AuthorizationCommitment(record.envelope); }
    catch { invalid("envelope"); }
    if (commitment !== id) invalid("commitment");
    let previous;
    for (const event of record.history) {
      if (!object(event) || !Object.hasOwn(TRANSITIONS, event.state) ||
          !Number.isSafeInteger(event.atUnix) || event.atUnix < 0 || Object.is(event.atUnix, -0)) invalid("history");
      if (!previous ? event.state !== "CREATED" :
          !TRANSITIONS[previous.state].includes(event.state) || event.atUnix < previous.atUnix) invalid("history transition");
      previous = event;
    }
    if (record.history[0].atUnix !== record.createdAtUnix || previous.atUnix !== record.updatedAtUnix ||
        previous.state !== record.state) invalid("history head");
    if (record.history.some(event => event.state === "AUTHORIZED")) {
      const domain = computeCrossL1ReplayDomainKey(record.envelope);
      const nonceKey = JSON.stringify([domain, record.envelope.nonce]);
      const intentKey = JSON.stringify([domain, record.envelope.intentId]);
      if (Object.hasOwn(nonces, nonceKey) || Object.hasOwn(intents, intentKey)) invalid("duplicate replay claim");
      nonces[nonceKey] = id; intents[intentKey] = id;
    }
  }
  for (const [actual, expected] of [[state.nonces, nonces], [state.intents, intents]]) {
    if (Object.keys(actual).length !== Object.keys(expected).length ||
        Object.entries(expected).some(([key, id]) => !Object.hasOwn(actual, key) || actual[key] !== id)) invalid("replay index mismatch");
  }
  return state;
}

/**
 * LOCAL PROTOTYPE ONLY. One private directory on a local filesystem supporting
 * atomic mkdir/rename and fsync; not NFS, multi-host consensus, or an RPC API.
 * The caller is a trusted orchestrator: it must authenticate cancellation,
 * administrative failures and submission reports. No signing/broadcast occurs.
 * Authorization is an explicitly trusted application integration boundary.
 * Evidence callbacks only validate data consistency: they cannot confirm or
 * settle. settleNative requires a fresh module-owned native RPC verification
 * handle, never a caller callback or deserialized persisted confirmation.
 *
 * A lock covers read/check/write of BOTH replay indexes and lifecycle record.
 * Replacement writes fsync file, rename, fsync directory before success. A crash
 * may leave .lock: FAIL CLOSED; operator must prove no writer is alive before
 * removing it. Never automatically steal locks. recover() then quarantines
 * ambiguous submission/legacy confirmation records without freeing replay keys.
 * State/replay-index consistency is validated on every read/write. This detects
 * partial corruption, not a malicious writer rewriting the complete history.
 * Storage rollback/deletion,
 * malicious local writers, hardware fsync lies, and lost-disk recovery are not
 * defended; production needs transactional replicated durable storage/backups.
 * The store retains full envelopes/evidence summaries (sensitive metadata) and
 * permanently retains replay keys. Use OS access controls/encryption externally.
 */
export class FileIntentStore {
  #root;
  #lockTimeoutMs;
  #nativeTrust;
  constructor(root, { lockTimeoutMs = 5000, nativeTrust = [] } = {}) {
    check(typeof root === "string" && root.length > 0, "store directory is required");
    check(Number.isSafeInteger(lockTimeoutMs) && lockTimeoutMs >= 0, "invalid lock timeout");
    this.#root = resolve(root);
    this.#lockTimeoutMs = lockTimeoutMs;
    // Operator-owned trust roots are independent of the request/receipt.
    this.#nativeTrust = trustSnapshot(nativeTrust);
  }

  async #read() {
    let raw;
    try { raw = await readFile(join(this.#root, "state.json"), "utf8"); }
    catch (error) {
      if (error.code === "ENOENT") return { version: 1, records: {}, nonces: {}, intents: {} };
      throw error;
    }
    return persistentState(JSON.parse(raw)); // corruption propagates; never silently reset
  }

  async #write(state) {
    persistentState(state);
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

  /**
   * Startup recovery requires exclusive orchestrator ownership: do not call this
   * while an external sender is still active. The normal filesystem lock is
   * required; an abandoned lock is never removed or stolen here. No send, retry,
   * proof verification, settlement, or replay-key release occurs during recovery.
   */
  async recover({ nowUnix } = {}) {
    const now = time(nowUnix);
    return this.#transaction(state => {
      const recovered = [];
      for (const record of Object.values(state.records)) {
        if (!["SUBMITTING", "SUBMITTED", "CONFIRMED", "SETTLED"].includes(record.state)) continue;
        this.#advance(record, "RECONCILING", [record.state], { nowUnix: now, reason: "crash-recovery" });
        delete record.confirmation;
        recovered.push(record);
      }
      return recovered;
    });
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
    check(verified.state === "EVIDENCE_VERIFIED", "NOT_VERIFIED: independent native settlement evidence required");
    check(verified.transactionId === prior.transactionId ||
      (prior.state === "RECONCILING" && prior.transactionId === undefined), "settlement transaction mismatch");
    return this.#update(id, "CONFIRMED", ["SUBMITTED", "RECONCILING"], { nowUnix, expectedRevision }, record => {
      record.transactionId = verified.transactionId;
      record.confirmation = verified;
    });
  }

  /** Atomically consume a fresh, native-verifier-owned receipt. No caller proof
   * callback or persisted confirmation can unlock this path. The operator RPC,
   * code pin and upgrade-authority trust assumptions remain explicit.
   */
  async settleNative(id, options = {}) {
    check(ID.test(id), "invalid intent record id");
    const handle = options.verification;
    const verified = readVerifiedNativeSettlement(handle);
    check(verified, "NOT_VERIFIED: fresh native verification handle required");
    check(verified.authorizationCommitment === id, "native authorization commitment mismatch");
    const prior = await this.get(id);
    const captured = { nowUnix: options.nowUnix, expectedRevision: options.expectedRevision ?? prior.revision };
    return this.#transaction(state => {
      // Check wall-clock freshness again after waiting for the filesystem lock.
      const proof = readVerifiedNativeSettlement(handle);
      check(proof, "NOT_VERIFIED: expired native verification handle");
      check(this.#nativeTrust.some(identity => TRUST_FIELDS.every(key =>
        identity[key] === proof.verifierIdentity?.[key])), "native verifier trust identity not approved by store operator");
      const record = state.records[id];
      check(record, "intent not found");
      check(proof.authorizationCommitment === record.id, "native authorization commitment mismatch");
      check(proof.sourceNetworkId === record.envelope.source.networkId &&
        proof.sourceNativeChainId === record.envelope.source.nativeChainId &&
        proof.destinationNetworkId === record.envelope.destination.networkId &&
        proof.destinationNativeChainId === record.envelope.destination.nativeChainId, "native network binding mismatch");
      check(record.transactionId === proof.transactionId ||
        (record.state === "RECONCILING" && record.transactionId === undefined), "native settlement transaction mismatch");
      check(!Object.values(state.records).some(other => other.id !== id &&
        other.transactionId === proof.transactionId), "native transaction already reserved by another intent");
      this.#advance(record, "CONFIRMED", ["SUBMITTED", "RECONCILING"], captured);
      record.transactionId = proof.transactionId;
      record.confirmation = structuredClone(proof);
      this.#advance(record, "SETTLED", ["CONFIRMED"], { nowUnix: captured.nowUnix, expectedRevision: record.revision });
      return record;
    });
  }

  async settle() {
    // Historical persisted CONFIRMED records may originate from caller callbacks.
    // Do not grandfather them into settlement without genuine native proof.
    throw new Error("NOT_IMPLEMENTED: native authorization-bound settlement and durable proof integration required");
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
