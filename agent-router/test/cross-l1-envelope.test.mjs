import test from "node:test";
import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import {
  canonicalizeCrossL1AuthorizationEnvelope,
  stringifyCrossL1AuthorizationEnvelope,
  canonicalCrossL1AuthorizationBytes,
  computeCrossL1ReplayDomainKey,
  computeCrossL1AuthorizationCommitment,
  authorizeCrossL1Envelope,
  verifyCrossL1SettlementEvidence,
} from "../src/cross-l1-envelope.mjs";

const BASE_TIME = 1_800_000_000;

function envelope() {
  return {
    schema: "qpc.cross-l1.authorization",
    version: 1,
    intentId: "intent-2026-0001",
    agentIdentity: {
      namespace: "qpc-agent",
      id: "agent-17",
      keyId: "agent-key-2026-01",
    },
    source: {
      networkId: "near:mainnet",
      nativeChainId: "mainnet",
      accountId: "agent.near",
      adapterId: "near-v1",
    },
    destination: {
      networkId: "zcash:testnet",
      nativeChainId: "testnet",
      adapterId: "zcash-shielded-v1",
      recipientCommitment: "a".repeat(64),
    },
    action: {
      type: "asset-transfer",
      operation: "pay",
    },
    asset: {
      sourceAssetId: "near:wrap.near",
      destinationAssetId: "zcash:zec",
      sourceDecimals: 24,
      destinationDecimals: 8,
      mappingId: "map-near-zec-v1",
    },
    amount: {
      sourceAtomic: "1000000000000000000000000",
      destinationAtomic: "123456789",
    },
    fees: {
      maxFeeAtomic: "1000",
      feeAssetId: "near:wrap.near",
      maxSlippageBps: 50,
    },
    nonce: "7",
    replayDomain: {
      authorityId: "qpc-near-mainnet",
      scope: "agent-payment",
    },
    policy: {
      id: "policy-1",
      version: 4,
      commitment: "b".repeat(64),
    },
    time: {
      issuedAtUnix: BASE_TIME,
      notBeforeUnix: BASE_TIME,
      expiresAtUnix: BASE_TIME + 300,
    },
    settlement: {
      evidenceClass: "shielded-note-proof-v1",
      finalityPolicy: "zcash-finalized-v1",
      minConfirmations: 10,
    },
    cancellation: {
      authorityId: "qpc-owner-key-1",
      revocationEpoch: 3,
    },
  };
}

function changedAt(path, value) {
  const copy = structuredClone(envelope());
  let target = copy;
  for (const key of path.slice(0, -1)) target = target[key];
  target[path.at(-1)] = value;
  return copy;
}

const mutations = [
  [["intentId"], "intent-2026-0002"],
  [["agentIdentity", "namespace"], "other-agent-namespace"],
  [["agentIdentity", "id"], "agent-18"],
  [["agentIdentity", "keyId"], "agent-key-2026-02"],
  [["source", "networkId"], "eip155:1"],
  [["source", "nativeChainId"], "testnet"],
  [["source", "accountId"], "0x1111111111111111111111111111111111111111"],
  [["source", "adapterId"], "evm-v1"],
  [["destination", "networkId"], "solana:mainnet-beta"],
  [["destination", "nativeChainId"], "mainnet"],
  [["destination", "adapterId"], "solana-v1"],
  [["destination", "recipientCommitment"], "c".repeat(64)],
  [["action", "type"], "contract-call"],
  [["action", "operation"], "approve"],
  [["asset", "sourceAssetId"], "near:usdc"],
  [["asset", "destinationAssetId"], "zcash:other-asset"],
  [["asset", "sourceDecimals"], 6],
  [["asset", "destinationDecimals"], 6],
  [["asset", "mappingId"], "map-near-usdc-v2"],
  [["amount", "sourceAtomic"], "2000000000000000000000000"],
  [["amount", "destinationAtomic"], "223456789"],
  [["fees", "maxFeeAtomic"], "1001"],
  [["fees", "feeAssetId"], "near:usdc"],
  [["fees", "maxSlippageBps"], 51],
  [["nonce"], "8"],
  [["replayDomain", "authorityId"], "qpc-evm-mainnet"],
  [["replayDomain", "scope"], "agent-withdrawal"],
  [["policy", "id"], "policy-2"],
  [["policy", "version"], 5],
  [["policy", "commitment"], "d".repeat(64)],
  [["time", "issuedAtUnix"], BASE_TIME - 1],
  [["time", "notBeforeUnix"], BASE_TIME + 1],
  [["time", "expiresAtUnix"], BASE_TIME + 301],
  [["settlement", "evidenceClass"], "evm-receipt-proof-v1"],
  [["settlement", "finalityPolicy"], "ethereum-finalized-v1"],
  [["settlement", "minConfirmations"], 11],
  [["cancellation", "authorityId"], "qpc-owner-key-2"],
  [["cancellation", "revocationEpoch"], 4],
];

test("canonical envelope produces stable 32-byte commitment independent of input key order", () => {
  const first = envelope();
  const reversed = Object.fromEntries(Object.entries(first).reverse());
  assert.equal(
    computeCrossL1AuthorizationCommitment(first),
    computeCrossL1AuthorizationCommitment(reversed),
  );
  assert.match(computeCrossL1AuthorizationCommitment(first), /^[0-9a-f]{64}$/);
  assert.equal(canonicalizeCrossL1AuthorizationEnvelope(first).schema, "qpc.cross-l1.authorization");
});

for (const [path, value] of mutations) {
  test(`authorization commitment binds ${path.join(".")}`, () => {
    assert.notEqual(
      computeCrossL1AuthorizationCommitment(envelope()),
      computeCrossL1AuthorizationCommitment(changedAt(path, value)),
    );
  });
}

test("shared literal canonical JSON and commitment match byte-for-byte", async () => {
  const fixture = JSON.parse(await readFile(new URL("../../docs/fixtures/cross-l1-v1.json", import.meta.url)));
  assert.equal(stringifyCrossL1AuthorizationEnvelope(fixture.envelope), fixture.canonicalJson);
  assert.equal(canonicalCrossL1AuthorizationBytes(fixture.envelope).toString("utf8"), fixture.canonicalJson);
  assert.equal(computeCrossL1AuthorizationCommitment(fixture.envelope), fixture.authorizationCommitment);
});

test("rejects invalid numeric bounds, non-data objects and nested unknown fields", () => {
  for (const [path, value] of [
    [["nonce"], "340282366920938463463374607431768211456"],
    [["amount", "sourceAtomic"], "9".repeat(1000)],
    [["time", "expiresAtUnix"], Number.MAX_SAFE_INTEGER + 1],
    [["policy", "version"], -0],
    [["asset", "sourceDecimals"], 256],
    [["fees", "maxSlippageBps"], 10001],
    [["source", "nativeChainId"], "é"],
  ]) assert.throws(() => canonicalizeCrossL1AuthorizationEnvelope(changedAt(path, value)));
  for (const field of Object.keys(envelope()).filter(k => typeof envelope()[k] === "object")) {
    const e = envelope(); e[field].extra = true;
    assert.throws(() => canonicalizeCrossL1AuthorizationEnvelope(e), /unexpected field/);
  }
  const e = envelope(); let reads = 0;
  Object.defineProperty(e.source, "accountId", { enumerable: true, get() { reads++; return "attacker"; } });
  assert.throws(() => canonicalizeCrossL1AuthorizationEnvelope(e), /data fields/);
  assert.equal(reads, 0);
  const hidden = envelope(); Object.defineProperty(hidden, "hidden", { value: true });
  assert.throws(() => canonicalizeCrossL1AuthorizationEnvelope(hidden), /data fields/);
  const symbol = envelope(); symbol[Symbol("extra")] = true;
  assert.throws(() => canonicalizeCrossL1AuthorizationEnvelope(symbol), /data fields/);
});

test("authorization snapshot and every nested field remain immutable across async verifier", async () => {
  const e = envelope();
  const expected = computeCrossL1AuthorizationCommitment(e);
  const result = await authorizeCrossL1Envelope(e, {
    nowUnix: BASE_TIME + 1,
    verifyAgentAuthorization: async ({ envelope: frozen }) => {
      assert.throws(() => { frozen.destination.recipientCommitment = "f".repeat(64); }, TypeError);
      e.destination.recipientCommitment = "f".repeat(64);
      await Promise.resolve();
      return true;
    },
    consumeReplay: async ({ authorizationCommitment }) => { assert.equal(authorizationCommitment, expected); return true; },
  });
  assert.equal(result.authorizationCommitment, expected);
});

test("replay domain tuple binds native chain and cannot reset on key rotation", () => {
  const original = computeCrossL1ReplayDomainKey(envelope());
  assert.notEqual(original, computeCrossL1ReplayDomainKey(changedAt(["source", "nativeChainId"], "testnet")));
  assert.equal(original, computeCrossL1ReplayDomainKey(changedAt(["agentIdentity", "keyId"], "rotated-key")));
  assert.equal(original, computeCrossL1ReplayDomainKey(changedAt(["policy", "version"], 9)));
  assert.throws(() => computeCrossL1ReplayDomainKey(changedAt(["replayDomain", "scope"], "contains|delimiter")));
});

test("settlement evidence snapshot cannot change during asynchronous verification", async () => {
  const mutable = settlementEvidence();
  const result = await verifyCrossL1SettlementEvidence({ authorization: envelope(), evidence: mutable,
    verifyDestinationEvidence: async ({ evidence: frozen }) => {
      assert.throws(() => { frozen.transactionId = "forged"; }, TypeError);
      mutable.transactionId = "forged";
      await Promise.resolve();
      return true;
    },
  });
  assert.equal(result.transactionId, "zcash-tx-0001");
});

test("rejects unknown critical fields instead of silently omitting them", () => {
  const extraTopLevel = { ...envelope(), unknownSecurityMode: "skip-checks" };
  const extraNested = envelope();
  extraNested.destination.unreviewedFallback = "transparent";
  assert.throws(() => canonicalizeCrossL1AuthorizationEnvelope(extraTopLevel), /unexpected field/);
  assert.throws(() => canonicalizeCrossL1AuthorizationEnvelope(extraNested), /unexpected field/);
});

test("rejects unsupported schema versions and non-canonical numeric strings", () => {
  assert.throws(
    () => canonicalizeCrossL1AuthorizationEnvelope({ ...envelope(), version: 2 }),
    /unsupported authorization schema or version/,
  );
  const leadingZeroAmount = envelope();
  leadingZeroAmount.amount.sourceAtomic = "01";
  assert.throws(() => canonicalizeCrossL1AuthorizationEnvelope(leadingZeroAmount), /canonical unsigned decimal/);
  const leadingZeroNonce = envelope();
  leadingZeroNonce.nonce = "07";
  assert.throws(() => canonicalizeCrossL1AuthorizationEnvelope(leadingZeroNonce), /nonce/);
});

test("rejects invalid time windows and malformed recipient commitments", () => {
  const expired = envelope();
  expired.time.expiresAtUnix = BASE_TIME;
  assert.throws(() => canonicalizeCrossL1AuthorizationEnvelope(expired), /time window/);
  const malformedRecipient = envelope();
  malformedRecipient.destination.recipientCommitment = "not-a-commitment";
  assert.throws(() => canonicalizeCrossL1AuthorizationEnvelope(malformedRecipient), /recipient commitment/);
});

test("authorization requires identity verification and a durable atomic replay consumer", async () => {
  const consumedNonces = new Set();
  const consumedIntents = new Set();
  const calls = [];
  const dependencies = {
    nowUnix: BASE_TIME + 1,
    verifyAgentAuthorization: async ({ authorizationCommitment }) => {
      calls.push(["verify", authorizationCommitment]);
      return true;
    },
    consumeReplay: async ({ replayDomainKey, nonce, intentId }) => {
      calls.push(["consume", replayDomainKey, nonce, intentId]);
      const nonceKey = `${replayDomainKey}:nonce:${nonce}`;
      const intentKey = `${replayDomainKey}:intent:${intentId}`;
      if (consumedNonces.has(nonceKey) || consumedIntents.has(intentKey)) return false;
      consumedNonces.add(nonceKey);
      consumedIntents.add(intentKey);
      return true;
    },
  };

  const first = await authorizeCrossL1Envelope(envelope(), dependencies);
  assert.equal(first.state, "AUTHORIZED");
  assert.match(first.authorizationCommitment, /^[0-9a-f]{64}$/);
  assert.equal(calls[0][0], "verify");
  assert.equal(calls[1][0], "consume");

  await assert.rejects(
    authorizeCrossL1Envelope(envelope(), dependencies),
    /already consumed/,
  );
  assert.equal(calls.filter(([kind]) => kind === "consume").length, 2);
});

test("does not consume a nonce when identity authentication fails", async () => {
  let consumeCalls = 0;
  await assert.rejects(
    authorizeCrossL1Envelope(envelope(), {
      nowUnix: BASE_TIME + 1,
      verifyAgentAuthorization: async () => false,
      consumeReplay: async () => {
        consumeCalls += 1;
        return true;
      },
    }),
    /agent authorization verification failed/,
  );
  assert.equal(consumeCalls, 0);
});

test("rejects authorization outside its not-before and expiry window", async () => {
  let verifyCalls = 0;
  const dependencies = {
    nowUnix: BASE_TIME - 1,
    verifyAgentAuthorization: async () => {
      verifyCalls += 1;
      return true;
    },
    consumeReplay: async () => true,
  };
  await assert.rejects(authorizeCrossL1Envelope(envelope(), dependencies), /not yet valid/);
  assert.equal(verifyCalls, 0);
});

function settlementEvidence(overrides = {}) {
  return {
    schema: "qpc.cross-l1.settlement-evidence",
    version: 1,
    authorizationCommitment: computeCrossL1AuthorizationCommitment(envelope()),
    evidenceClass: "shielded-note-proof-v1",
    destinationNetworkId: "zcash:testnet",
    destinationAdapterId: "zcash-shielded-v1",
    destinationAssetId: "zcash:zec",
    amountAtomic: "123456789",
    recipientCommitment: "a".repeat(64),
    transactionId: "zcash-tx-0001",
    inclusionReference: "zcash-height-2500000",
    confirmations: 12,
    finalityStatus: "FINALIZED",
    proofDigest: "e".repeat(64),
    observedAtUnix: BASE_TIME + 400,
    ...overrides,
  };
}

test("settlement evidence must bind the exact authorization and pass the chain-specific verifier", async () => {
  let verifierCalls = 0;
  const result = await verifyCrossL1SettlementEvidence({
    authorization: envelope(),
    evidence: settlementEvidence(),
    verifyDestinationEvidence: async ({ authorizationCommitment, evidence }) => {
      verifierCalls += 1;
      assert.equal(authorizationCommitment, evidence.authorizationCommitment);
      assert.equal(evidence.transactionId, "zcash-tx-0001");
      return true;
    },
  });
  assert.equal(result.state, "NOT_VERIFIED");
  assert.match(result.reason, /not independent native/);
  assert.equal(result.transactionId, "zcash-tx-0001");
  assert.equal(verifierCalls, 1);
});

for (const [field, value, message] of [
  ["authorizationCommitment", "f".repeat(64), /authorization commitment/],
  ["destinationNetworkId", "eip155:1", /destination network/],
  ["destinationAdapterId", "evm-v1", /destination adapter/],
  ["destinationAssetId", "zcash:other", /destination asset/],
  ["amountAtomic", "1", /amount/],
  ["recipientCommitment", "c".repeat(64), /recipient commitment/],
  ["evidenceClass", "public-tx-hash-only", /evidence class/],
  ["confirmations", 1, /confirmation threshold/],
]) {
  test(`rejects settlement evidence mismatch in ${field}`, async () => {
    let verifierCalls = 0;
    await assert.rejects(
      verifyCrossL1SettlementEvidence({
        authorization: envelope(),
        evidence: settlementEvidence({ [field]: value }),
        verifyDestinationEvidence: async () => {
          verifierCalls += 1;
          return true;
        },
      }),
      message,
    );
    assert.equal(verifierCalls, 0);
  });
}

test("rejects chain-specific verifier failure and never equates provider data with settlement", async () => {
  await assert.rejects(
    verifyCrossL1SettlementEvidence({
      authorization: envelope(),
      evidence: settlementEvidence(),
      verifyDestinationEvidence: async () => false,
    }),
    /chain-specific evidence verification failed/,
  );
});
