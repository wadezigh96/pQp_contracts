import test from "node:test";
import assert from "node:assert/strict";
import {
  prepareNearToEthereumPayment, submitNearDeposit, verifyPaymentStatus,
} from "../src/index.mjs";

const tokens = [
  { assetId: "nep141:test-token.near", blockchain: "near", symbol: "TST", decimals: 6, contractAddress: "test-token.near" },
  { assetId: "eth:0xtest", blockchain: "eth", symbol: "TST", decimals: 6, contractAddress: "0x0000000000000000000000000000000000000001" },
];
const quote = {
  correlationId: "qpc-test-quote-1",
  quote: {
    depositAddress: "deposit.near", amountIn: "1000000",
    amountOut: "950000", minAmountOut: "940000",
    deadline: new Date(Date.now() + 60000).toISOString(),
  },
};
function mockFetch() {
  const calls = [];
  const fetchImpl = async (url, options = {}) => {
    calls.push({ url: String(url), options });
    if (String(url).endsWith("/v0/tokens"))
      return new Response(JSON.stringify(tokens), { status: 200 });
    if (String(url).endsWith("/v0/quote"))
      return new Response(JSON.stringify(quote), { status: 200 });
    if (String(url).includes("/v0/deposit/submit"))
      return new Response(JSON.stringify({ accepted: true }), { status: 200 });
    throw new Error("unexpected URL");
  };
  return { fetchImpl, calls };
}
const validInput = {
  originAssetId: tokens[0].assetId, destinationAssetId: tokens[1].assetId,
  amount: "1000000",
  recipient: "0x0000000000000000000000000000000000000002",
  refundTo: "agent.near", policyGate: async () => true,
};

test("prepares NEAR -> Ethereum NEP-141 deposit without signing", async () => {
  const { fetchImpl } = mockFetch();
  const prepared = await prepareNearToEthereumPayment({ ...validInput, fetchImpl });
  assert.equal(prepared.state, "AWAITING_SOURCE_SIGNATURE");
  assert.equal(prepared.sourceTransaction.receiverId, "test-token.near");
  assert.deepEqual(prepared.sourceTransaction.actions[0].params.args, {
    receiver_id: "deposit.near", amount: "1000000",
  });
  assert.equal(prepared.sourceTransaction.actions[0].params.deposit, "1");
});

test("requires QPC policy approval before and after quote commitment", async () => {
  const { fetchImpl } = mockFetch();
  const phases = [];
  const prepared = await prepareNearToEthereumPayment({
    ...validInput, fetchImpl,
    policyGate: async ({ phase }) => { phases.push(phase); return true; },
  });
  assert.deepEqual(phases, ["preflight", "committed_quote"]);
  assert.equal(prepared.state, "AWAITING_SOURCE_SIGNATURE");
});

test("rejects when QPC policy gate denies the intent", async () => {
  const { fetchImpl } = mockFetch();
  await assert.rejects(
    prepareNearToEthereumPayment({ ...validInput, fetchImpl, policyGate: async () => false }),
    /QPC policy rejected/,
  );
});

test("rejects wrong source chain and invalid recipient", async () => {
  const { fetchImpl } = mockFetch();
  await assert.rejects(
    prepareNearToEthereumPayment({ ...validInput, originAssetId: tokens[1].assetId, fetchImpl }),
    /origin asset must be on near/,
  );
  await assert.rejects(
    prepareNearToEthereumPayment({ ...validInput, recipient: "not-an-eth-address", fetchImpl }),
    /valid Ethereum address/,
  );
});

test("wallet submission requires explicit signer and returns tx hash", async () => {
  const { fetchImpl } = mockFetch();
  const prepared = await prepareNearToEthereumPayment({ ...validInput, fetchImpl });
  const submitted = await submitNearDeposit(prepared, {
    fetchImpl, nearSenderAccount: "agent.near",
    signAndSendTransaction: async (tx) => {
      assert.equal(tx.receiverId, "test-token.near");
      return { transaction: { hash: "near-test-tx-hash" } };
    },
  });
  assert.equal(submitted.state, "SOURCE_SUBMITTED");
  assert.equal(submitted.txHash, "near-test-tx-hash");
});

test("provider SUCCESS is not settled without destination verification", async () => {
  const status = { status: "SUCCESS", quoteResponse: { quoteRequest: {}, quote: {} } };
  await assert.rejects(verifyPaymentStatus(status), /independent destination-chain verification/);
  const result = await verifyPaymentStatus(status, async () => true);
  assert.equal(result.state, "SETTLED");
});

test("failed and refunded statuses remain terminal failures", async () => {
  assert.deepEqual(await verifyPaymentStatus({ status: "REFUNDED" }), { state: "REFUNDED" });
  assert.deepEqual(await verifyPaymentStatus({ status: "FAILED" }), { state: "FAILED" });
});
