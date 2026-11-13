import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  buildNearToZcashShieldedDispatch,
  computeNearToZcashPolicyArgsHash,
} from "../src/index.mjs";

const baseRequest = {
  protocol: "QPC_NEAR_ZCASH_SHIELDED_V1",
  payment_id: "pay-2026-0001",
  destination: "zcash",
  network: "testnet",
  opaque_recipient_ref: "ref-7f2a9c8405d1b6e3f48a2d0c",
  privacy_class: "shielded",
  amount_zatoshis: "25000",
  expires_at_unix: 1_800_000_300,
};

function hashPayload(payload = baseRequest) {
  return createHash("sha256").update(JSON.stringify(payload), "utf8").digest("hex");
}

function build(overrides = {}) {
  return buildNearToZcashShieldedDispatch({
    qpcContractId: "qpc.testnet",
    policyId: "near_to_zcash",
    policyVersion: 3,
    requestId: "near-zcash-req-1",
    nonce: 7,
    actionId: "pay_zcash_shielded",
    zcashRouterAccountId: "zcash-router.testnet",
    paymentId: baseRequest.payment_id,
    opaqueRecipientRef: baseRequest.opaque_recipient_ref,
    amountZatoshis: baseRequest.amount_zatoshis,
    expiresAtUnix: baseRequest.expires_at_unix,
    nowUnix: 1_800_000_000,
    policyArgsHash: hashPayload(),
    ...overrides,
  });
}

test("builds a deterministic Testnet shielded QPC dispatch with only an opaque destination reference", () => {
  const prepared = build();
  assert.equal(prepared.state, "PREPARED_LOCAL_ONLY");
  assert.equal(prepared.network, "testnet");
  assert.equal(prepared.privacyClass, "shielded");
  assert.equal(prepared.qpcCall.receiverId, "qpc.testnet");
  assert.equal(prepared.qpcCall.methodName, "dispatch_action");
  assert.equal(prepared.qpcCall.args.target, "zcash-router.testnet");
  assert.equal(prepared.qpcCall.args.method_name, "request_shielded_payment");
  assert.equal(prepared.qpcCall.args.attached_deposit, 0);
  assert.deepEqual(prepared.zcashRequest, baseRequest);
  assert.equal(prepared.policyArgsHash, hashPayload());

  const decoded = Buffer.from(prepared.qpcCall.args.args).toString("utf8");
  assert.equal(decoded, JSON.stringify(baseRequest));
  assert.ok(!decoded.includes("u1"));
  assert.match(decoded, /\"privacy_class\":\"shielded\"/);
});

test("computes the exact canonical argument hash for QPC ActionRule configuration", () => {
  assert.equal(computeNearToZcashPolicyArgsHash({
    paymentId: baseRequest.payment_id,
    opaqueRecipientRef: baseRequest.opaque_recipient_ref,
    amountZatoshis: baseRequest.amount_zatoshis,
    expiresAtUnix: baseRequest.expires_at_unix,
    nowUnix: 1_800_000_000,
  }), hashPayload());
});

test("rejects payload not bound to the configured QPC action hash", () => {
  assert.throws(() => build({ amountZatoshis: "25001" }), /does not match the configured QPC policy args hash/);
});

test("rejects raw or low-entropy recipient references", () => {
  assert.throws(() => build({ opaqueRecipientRef: "u1exampleaddress" }), /high-entropy ref-/);
  assert.throws(() => build({ opaqueRecipientRef: "ref-7f2a" }), /high-entropy ref-/);
});

test("rejects mainnet and any transparent privacy downgrade", () => {
  assert.throws(() => build({ network: "mainnet" }), /only Zcash Testnet is enabled/);
  assert.throws(() => build({ privacyClass: "transparent" }), /only shielded privacy class/);
});

test("rejects expired or long-lived requests", () => {
  assert.throws(() => build({ nowUnix: baseRequest.expires_at_unix }), /expired/);
  assert.throws(() => build({ expiresAtUnix: 1_800_004_000 }), /one-hour limit/);
});

test("rejects invalid and overflowing zatoshi amounts", () => {
  assert.throws(() => build({ amountZatoshis: "0" }), /positive decimal integer/);
  assert.throws(() => build({ amountZatoshis: "01" }), /positive decimal integer/);
  assert.throws(() => build({ amountZatoshis: "18446744073709551616" }), /u64 range/);
});

test("preparation makes no signing or settlement claim", () => {
  const prepared = build();
  assert.equal(prepared.state, "PREPARED_LOCAL_ONLY");
  assert.ok(prepared.warnings.some((warning) => warning.includes("No zcash-router.near deployment")));
  assert.ok(prepared.warnings.some((warning) => warning.includes("No on-chain policy was queried")));
  assert.throws(() => { prepared.qpcCall.args.args[0] = 0; }, TypeError);
});
