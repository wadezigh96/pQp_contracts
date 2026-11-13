import test from "node:test";
import assert from "node:assert/strict";
import { authorizeX402NearExactOffer } from "../src/index.mjs";

const paymentRequired = {
  x402Version: 2,
  resource: {
    url: "https://api.example.test/reports/daily",
    description: "Daily report",
    mimeType: "application/json",
  },
  accepts: [{
    scheme: "exact",
    network: "near:testnet",
    amount: "250000",
    asset: "usdc.testnet",
    payTo: "merchant.testnet",
    maxTimeoutSeconds: 60,
  }],
};

const policy = {
  network: "near:testnet",
  allowedAssets: ["usdc.testnet"],
  allowedRecipients: ["merchant.testnet"],
  allowedResourceOrigins: ["https://api.example.test"],
  maxAmount: "500000",
  maxTimeoutSeconds: 90,
};

function invoke(overrides = {}) {
  return authorizeX402NearExactOffer({
    paymentRequired,
    acceptedIndex: 0,
    policy,
    policyGate: async () => true,
    ...overrides,
  });
}

test("approves an exact NEAR testnet offer locally and returns no payment artifact", async () => {
  const calls = [];
  const result = await invoke({
    policyGate: async (context) => { calls.push(context); return true; },
  });

  assert.equal(result.state, "POLICY_APPROVED_LOCAL_ONLY");
  assert.equal(calls.length, 1);
  assert.equal(calls[0].phase, "x402_preflight");
  assert.equal(calls[0].intent.network, "near:testnet");
  assert.equal(calls[0].intent.amount, "250000");
  assert.equal(calls[0].intent.resourceUrl, "https://api.example.test/reports/daily");
  assert.equal("transaction" in result, false);
  assert.equal("signedDelegateAction" in result, false);
  assert.equal("settlement" in result, false);
  assert.match(result.warnings.join(" "), /No x402 facilitator was contacted/);
});

test("rejects an x402 v1 or malformed PaymentRequired before policy approval", async () => {
  let policyCalls = 0;
  const policyGate = async () => { policyCalls += 1; return true; };
  await assert.rejects(invoke({
    paymentRequired: { ...paymentRequired, x402Version: 1 }, policyGate,
  }), /only x402 v2/);
  await assert.rejects(invoke({
    paymentRequired: { ...paymentRequired, accepts: [] }, policyGate,
  }), /non-empty array/);
  assert.equal(policyCalls, 0);
});

test("rejects unsupported scheme and network", async () => {
  await assert.rejects(invoke({
    paymentRequired: { ...paymentRequired, accepts: [{ ...paymentRequired.accepts[0], scheme: "upto" }] },
  }), /only the x402 exact scheme/);
  await assert.rejects(invoke({
    paymentRequired: { ...paymentRequired, accepts: [{ ...paymentRequired.accepts[0], network: "near:mainnet" }] },
  }), /network is not allowed by policy/);
});

test("rejects unapproved asset, recipient, and resource origin", async () => {
  await assert.rejects(invoke({
    paymentRequired: { ...paymentRequired, accepts: [{ ...paymentRequired.accepts[0], asset: "other.testnet" }] },
  }), /asset is not allowlisted/);
  await assert.rejects(invoke({
    paymentRequired: { ...paymentRequired, accepts: [{ ...paymentRequired.accepts[0], payTo: "attacker.testnet" }] },
  }), /recipient is not allowlisted/);
  await assert.rejects(invoke({
    paymentRequired: {
      ...paymentRequired,
      resource: { ...paymentRequired.resource, url: "https://evil.example/reports" },
    },
  }), /resource origin is not allowlisted/);
});

test("rejects an amount above policy and malformed atomic amounts", async () => {
  await assert.rejects(invoke({
    paymentRequired: { ...paymentRequired, accepts: [{ ...paymentRequired.accepts[0], amount: "500001" }] },
  }), /amount exceeds the policy limit/);
  for (const amount of ["0", "-1", "01", "1.5", "1e6", "340282366920938463463374607431768211456"]) {
    await assert.rejects(invoke({
      paymentRequired: { ...paymentRequired, accepts: [{ ...paymentRequired.accepts[0], amount }] },
    }));
  }
});

test("rejects invalid or overlong x402 timeout bounds", async () => {
  for (const maxTimeoutSeconds of [0, -1, 91, 1.5, "60"]) {
    await assert.rejects(invoke({
      paymentRequired: {
        ...paymentRequired,
        accepts: [{ ...paymentRequired.accepts[0], maxTimeoutSeconds }],
      },
    }), /timeout/);
  }
});

test("requires explicit bounds and a policy callback that returns true", async () => {
  await assert.rejects(invoke({ policy: undefined }), /explicit local x402 policy/);
  await assert.rejects(invoke({ policyGate: undefined }), /QPC policy gate is required/);
  await assert.rejects(invoke({ policyGate: async () => false }), /QPC policy rejected/);
  await assert.rejects(invoke({ policyGate: async () => undefined }), /QPC policy rejected/);
});

test("rejects invalid resource URLs and an out-of-range offer index", async () => {
  await assert.rejects(invoke({ acceptedIndex: 2 }), /acceptedIndex is out of range/);
  await assert.rejects(invoke({
    paymentRequired: {
      ...paymentRequired,
      resource: { ...paymentRequired.resource, url: "http://api.example.test/reports" },
    },
  }), /must use HTTPS/);
});

test("does not invoke caller-supplied signer or facilitator functions", async () => {
  let signerCalls = 0;
  let facilitatorCalls = 0;
  const result = await authorizeX402NearExactOffer({
    paymentRequired,
    acceptedIndex: 0,
    policy,
    policyGate: async () => true,
    signAndSendTransaction: async () => { signerCalls += 1; },
    facilitator: { settle: async () => { facilitatorCalls += 1; } },
  });
  assert.equal(signerCalls, 0);
  assert.equal(facilitatorCalls, 0);
  assert.equal(result.state, "POLICY_APPROVED_LOCAL_ONLY");
});
