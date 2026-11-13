import test from "node:test";
import assert from "node:assert/strict";
import {
  buildNearToZcashShieldedDispatch,
  computeNearToZcashPolicyArgsHash,
  submitNearToZcashDispatch,
  verifyNearToZcashRoute,
} from "../src/index.mjs";

const request = {
  paymentId: "pay-2026-0001",
  opaqueRecipientRef: "ref-7f2a9c8405d1b6e3f48a2d0c",
  amountZatoshis: "25000",
  expiresAtUnix: 1_800_000_300,
  nowUnix: 1_800_000_000,
};

function prepared() {
  const policyArgsHash = computeNearToZcashPolicyArgsHash(request);
  return buildNearToZcashShieldedDispatch({
    ...request,
    qpcContractId: "qpc.testnet",
    policyId: "near_to_zcash",
    policyVersion: 3,
    requestId: "near-zcash-req-1",
    nonce: 7,
    actionId: "pay_zcash_shielded",
    zcashRouterAccountId: "zcash-router.testnet",
    policyArgsHash,
  });
}

test("submits only through explicit NEAR signer and route provider", async () => {
  const calls = [];
  const result = await submitNearToZcashDispatch(prepared(), {
    signAndSendNear: async (transaction) => {
      calls.push(["near", transaction]);
      return { transaction: { hash: "near-tx-1" } };
    },
    submitRoute: async (routeRequest) => {
      calls.push(["route", routeRequest]);
      return { routeId: "route-1", state: "PENDING" };
    },
  });

  assert.equal(result.state, "ROUTE_SUBMITTED");
  assert.equal(result.nearTxHash, "near-tx-1");
  assert.equal(result.routeId, "route-1");
  assert.equal(calls[0][1].methodName, "dispatch_action");
  assert.equal(calls[1][1].nearTxHash, "near-tx-1");
  assert.equal(calls[1][1].request.amount_zatoshis, "25000");
});

test("refuses route submission without explicit signer or provider", async () => {
  await assert.rejects(
    submitNearToZcashDispatch(prepared(), { submitRoute: async () => ({ routeId: "x" }) }),
    /explicit NEAR wallet signer/,
  );
  await assert.rejects(
    submitNearToZcashDispatch(prepared(), { signAndSendNear: async () => ({ transactionHash: "x" }) }),
    /explicit Zcash route provider/,
  );
});

test("does not mark a pending route as settled", async () => {
  const result = await verifyNearToZcashRoute(
    { routeId: "route-1", state: "PENDING" },
    async () => true,
  );
  assert.equal(result.state, "PENDING");
});

test("rejects provider SUCCESS when it changes approved payment fields", async () => {
  const expected = {
    paymentId: request.paymentId,
    amountZatoshis: request.amountZatoshis,
    opaqueRecipientRef: request.opaqueRecipientRef,
  };
  for (const changed of [
    { paymentId: "attacker-payment" },
    { amountZatoshis: "1" },
    { opaqueRecipientRef: "ref-attacker-123456789012345678" },
  ]) {
    const status = {
      routeId: "route-1",
      state: "SUCCESS",
      paymentId: request.paymentId,
      amountZatoshis: request.amountZatoshis,
      opaqueRecipientRef: request.opaqueRecipientRef,
      ...changed,
    };
    await assert.rejects(
      verifyNearToZcashRoute(status, expected, async () => true),
      /does not match approved intent/,
    );
  }
});

test("requires independent receipt verification for provider success", async () => {
  const status = {
    routeId: "route-1",
    state: "SUCCESS",
    paymentId: "pay-2026-0001",
    amountZatoshis: "25000",
    opaqueRecipientRef: request.opaqueRecipientRef,
  };
  const expected = {
    paymentId: request.paymentId,
    amountZatoshis: request.amountZatoshis,
    opaqueRecipientRef: request.opaqueRecipientRef,
  };
  await assert.rejects(
    verifyNearToZcashRoute(status, expected),
    /independent Zcash receipt verification/,
  );
  await assert.rejects(
    verifyNearToZcashRoute(status, expected, async () => false),
    /receipt verification failed/,
  );
  const settled = await verifyNearToZcashRoute(status, expected, async (receiptExpectation) => {
    assert.equal(receiptExpectation.expectedPaymentId, request.paymentId);
    assert.equal(receiptExpectation.expectedAmountZatoshis, request.amountZatoshis);
    assert.equal(receiptExpectation.expectedRecipientRef, request.opaqueRecipientRef);
    return true;
  });
  assert.equal(settled.state, "SETTLED");
});
