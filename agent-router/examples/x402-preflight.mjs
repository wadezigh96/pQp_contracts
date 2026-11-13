import { authorizeX402NearExactOffer } from "../src/index.mjs";

const paymentRequired = {
  x402Version: 2,
  resource: {
    url: "https://api.example.test/reports/daily",
    description: "Demo data report",
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

const decision = await authorizeX402NearExactOffer({
  paymentRequired,
  acceptedIndex: 0,
  policy: {
    network: "near:testnet",
    allowedAssets: ["usdc.testnet"],
    allowedRecipients: ["merchant.testnet"],
    allowedResourceOrigins: ["https://api.example.test"],
    maxAmount: "500000",
    maxTimeoutSeconds: 90,
  },
  policyGate: async ({ intent }) =>
    intent.network === "near:testnet" &&
    intent.resourceOrigin === "https://api.example.test" &&
    BigInt(intent.amount) <= 500000n,
});

console.log(JSON.stringify(decision, null, 2));
console.log("DEMO_ONLY: no signature, facilitator call, transaction, or payment was created.");
