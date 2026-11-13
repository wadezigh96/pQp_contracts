const API_BASE = "https://1click.chaindefuser.com";
const TERMINAL = new Set(["SUCCESS", "FAILED", "REFUNDED", "INCOMPLETE_DEPOSIT"]);

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
function isNearAccountId(value) {
  return typeof value === "string" && value.length >= 2 && value.length <= 64 &&
    /^[a-z0-9._-]+$/.test(value) && !value.startsWith(".") &&
    !value.endsWith(".") && !value.includes("..");
}
function isEvmAddress(value) {
  return typeof value === "string" && /^0x[0-9a-fA-F]{40}$/.test(value);
}
async function readJson(response, label) {
  const body = await response.text();
  let parsed;
  try { parsed = body ? JSON.parse(body) : {}; }
  catch { throw new Error(label + ": API returned invalid JSON"); }
  if (!response.ok) {
    const detail = typeof parsed.message === "string" ? parsed.message : response.statusText;
    throw new Error(label + ": HTTP " + response.status + " " + detail);
  }
  return parsed;
}
async function requestJson(fetchImpl, url, options, label) {
  return readJson(await fetchImpl(url, options), label);
}
function requirePolicyGate(policyGate) {
  assert(typeof policyGate === "function",
    "A QPC policy gate is required; refusing to prepare a payment without policy authorization");
}
function validateToken(token, chain, role) {
  assert(token && typeof token.assetId === "string" && token.assetId.length > 0,
    role + " assetId is missing");
  assert(token.blockchain === chain,
    role + " asset must be on " + chain + ", received " + token.blockchain);
  assert(Number.isInteger(token.decimals) && token.decimals >= 0 && token.decimals <= 30,
    role + " token decimals are invalid");
}

export async function fetchSupportedTokens({ fetchImpl = fetch } = {}) {
  const tokens = await requestJson(fetchImpl, API_BASE + "/v0/tokens", {}, "NEAR Intents tokens");
  assert(Array.isArray(tokens), "NEAR Intents token response must be an array");
  return tokens;
}

/** Prepare (but do not sign or send) a NEAR -> Ethereum payment. */
export async function prepareNearToEthereumPayment({
  originAssetId, destinationAssetId, amount, recipient, refundTo,
  slippageTolerance = 100, policyGate, fetchImpl = fetch,
}) {
  requirePolicyGate(policyGate);
  assert(typeof amount === "string" && /^[1-9][0-9]*$/.test(amount),
    "amount must be a positive integer in the token's smallest unit");
  assert(isEvmAddress(recipient), "recipient must be a valid Ethereum address");
  assert(isNearAccountId(refundTo), "refundTo must be a valid NEAR account ID");
  assert(Number.isInteger(slippageTolerance) && slippageTolerance >= 0 && slippageTolerance <= 500,
    "slippageTolerance must be between 0 and 500 bps");

  const tokens = await fetchSupportedTokens({ fetchImpl });
  const origin = tokens.find((token) => token.assetId === originAssetId);
  const destination = tokens.find((token) => token.assetId === destinationAssetId);
  validateToken(origin, "near", "origin");
  validateToken(destination, "eth", "destination");
  assert(typeof origin.contractAddress === "string" && origin.contractAddress.length > 0,
    "NEAR origin token must expose its NEP-141 contract address");

  const intent = Object.freeze({
    protocol: "QPC_AGENT_PAYMENT_V1",
    sourceChain: "near", destinationChain: "eth",
    originAssetId: origin.assetId, destinationAssetId: destination.assetId,
    amount, recipient, refundTo, slippageTolerance,
  });
  const preflight = await policyGate({ phase: "preflight", intent, origin, destination });
  assert(preflight === true, "QPC policy rejected this payment intent");

  const quote = await requestJson(fetchImpl, API_BASE + "/v0/quote", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      dry: false, swapType: "EXACT_INPUT",
      originAsset: origin.assetId, destinationAsset: destination.assetId,
      amount, recipient, refundTo, slippageTolerance,
    }),
  }, "NEAR Intents quote");

  assert(typeof quote.correlationId === "string" && quote.correlationId.length > 0,
    "quote correlationId is missing");
  assert(quote.quote && typeof quote.quote === "object", "quote payload is missing");
  const q = quote.quote;
  assert(typeof q.depositAddress === "string" && isNearAccountId(q.depositAddress),
    "NEAR quote depositAddress is invalid");
  assert(q.amountIn === amount, "quote amountIn differs from the authorized exact input");
  assert(typeof q.amountOut === "string" && /^[0-9]+$/.test(q.amountOut),
    "quote amountOut is invalid");
  assert(typeof q.minAmountOut === "string" && /^[0-9]+$/.test(q.minAmountOut),
    "quote minAmountOut is missing or invalid");
  assert(typeof q.deadline === "string" && Number.isFinite(Date.parse(q.deadline)),
    "quote deadline is missing or invalid");
  assert(Date.parse(q.deadline) > Date.now(), "quote is already expired");

  const committedIntent = Object.freeze({
    ...intent, depositAddress: q.depositAddress, amountIn: q.amountIn,
    amountOut: q.amountOut, minAmountOut: q.minAmountOut,
    deadline: q.deadline, correlationId: quote.correlationId,
  });
  const authorized = await policyGate({
    phase: "committed_quote", intent: committedIntent, origin, destination,
  });
  assert(authorized === true, "QPC policy rejected the committed quote");

  return Object.freeze({
    state: "AWAITING_SOURCE_SIGNATURE",
    intent: committedIntent,
    quote,
    sourceTransaction: Object.freeze({
      receiverId: origin.contractAddress,
      actions: [{
        type: "FunctionCall",
        params: {
          methodName: "ft_transfer",
          args: { receiver_id: q.depositAddress, amount: q.amountIn },
          gas: "50000000000000",
          deposit: "1",
        },
      }],
    }),
    warning: "Unsigned NEAR wallet transaction. No funds have been sent.",
  });
}

/** Submit only through an explicitly supplied NEAR wallet signer. */
export async function submitNearDeposit(prepared, {
  signAndSendTransaction, nearSenderAccount, fetchImpl = fetch,
}) {
  assert(prepared?.state === "AWAITING_SOURCE_SIGNATURE",
    "payment is not awaiting a source signature");
  assert(typeof signAndSendTransaction === "function",
    "an explicit NEAR wallet signer is required");
  assert(isNearAccountId(nearSenderAccount), "nearSenderAccount is invalid");
  assert(Date.parse(prepared.intent.deadline) > Date.now(), "quote expired before signing");

  const result = await signAndSendTransaction({
    receiverId: prepared.sourceTransaction.receiverId,
    actions: prepared.sourceTransaction.actions,
  });
  const txHash = result?.transaction?.hash ??
    result?.transaction_outcome?.id ?? result?.transactionHash;
  assert(typeof txHash === "string" && txHash.length > 0,
    "NEAR wallet did not return a transaction hash");

  await requestJson(fetchImpl, API_BASE + "/v0/deposit/submit", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      depositAddress: prepared.intent.depositAddress, txHash, nearSenderAccount,
    }),
  }, "NEAR Intents deposit notification");

  return Object.freeze({
    state: "SOURCE_SUBMITTED", txHash,
    depositAddress: prepared.intent.depositAddress,
    correlationId: prepared.intent.correlationId,
  });
}

export async function getPaymentStatus(depositAddress, { fetchImpl = fetch } = {}) {
  assert(isNearAccountId(depositAddress), "depositAddress is invalid");
  const url = new URL(API_BASE + "/v0/status");
  url.searchParams.set("depositAddress", depositAddress);
  return requestJson(fetchImpl, url.toString(), {}, "NEAR Intents status");
}

/** Provider SUCCESS requires independent destination receipt verification. */
export async function verifyPaymentStatus(status, verifyDestination) {
  assert(status && typeof status.status === "string", "status response is invalid");
  if (!TERMINAL.has(status.status)) return { state: status.status };
  if (status.status !== "SUCCESS") return { state: status.status };
  assert(typeof verifyDestination === "function",
    "independent destination-chain verification is required before marking SETTLED");
  const verified = await verifyDestination({
    status,
    expectedRecipient: status.quoteResponse?.quoteRequest?.recipient,
    expectedDestinationAsset: status.quoteResponse?.quoteRequest?.destinationAsset,
    expectedMinimumAmount: status.quoteResponse?.quote?.minAmountOut,
  });
  assert(verified === true, "destination-chain receipt verification failed");
  return { state: "SETTLED", status };
}

export const QPC_PAYMENT_TERMINAL_STATES = Object.freeze([
  "SETTLED", "FAILED", "REFUNDED", "INCOMPLETE_DEPOSIT",
]);

export {
  buildNearToZcashShieldedDispatch,
  computeNearToZcashPolicyArgsHash,
} from "./near-zcash-intent.mjs";

export {
  submitNearToZcashDispatch,
  verifyNearToZcashRoute,
} from "./zcash-route.mjs";

export { authorizeX402NearExactOffer } from "./x402-policy.mjs";
