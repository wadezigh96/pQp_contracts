const NEAR_NETWORKS = new Set(["near:mainnet", "near:testnet"]);
const MAX_U128 = (1n << 128n) - 1n;
const NEAR_ACCOUNT_PATTERN = /^[a-z0-9._-]{2,64}$/;

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function isNearAccountId(value) {
  return typeof value === "string" && NEAR_ACCOUNT_PATTERN.test(value) &&
    !value.startsWith(".") && !value.endsWith(".") && !value.includes("..");
}

function parsePositiveAtomicAmount(value, label) {
  assert(typeof value === "string" && /^[1-9][0-9]*$/.test(value),
    `${label} must be a positive decimal integer string`);
  const parsed = BigInt(value);
  assert(parsed <= MAX_U128, `${label} exceeds the supported u128 range`);
  return parsed;
}

function normalizeHttpsOrigin(value, label) {
  assert(typeof value === "string" && value.length > 0, `${label} is invalid`);
  let parsed;
  try { parsed = new URL(value); }
  catch { throw new Error(`${label} must be a valid HTTPS origin`); }
  assert(parsed.protocol === "https:" && parsed.username === "" && parsed.password === "" &&
    parsed.pathname === "/" && parsed.search === "" && parsed.hash === "",
  `${label} must be a bare HTTPS origin`);
  return parsed.origin;
}

function validatePolicy(policy) {
  assert(policy && typeof policy === "object", "an explicit local x402 policy is required");
  assert(NEAR_NETWORKS.has(policy.network), "policy.network must be near:mainnet or near:testnet");
  assert(Array.isArray(policy.allowedAssets) && policy.allowedAssets.length > 0 &&
    policy.allowedAssets.every(isNearAccountId), "policy.allowedAssets must contain NEAR token contract IDs");
  assert(Array.isArray(policy.allowedRecipients) && policy.allowedRecipients.length > 0 &&
    policy.allowedRecipients.every(isNearAccountId), "policy.allowedRecipients must contain NEAR account IDs");
  assert(Array.isArray(policy.allowedResourceOrigins) && policy.allowedResourceOrigins.length > 0,
    "policy.allowedResourceOrigins must contain at least one HTTPS origin");
  const allowedResourceOrigins = policy.allowedResourceOrigins.map((origin) =>
    normalizeHttpsOrigin(origin, "policy.allowedResourceOrigins entry"));
  const maxAmount = parsePositiveAtomicAmount(policy.maxAmount, "policy.maxAmount");
  assert(Number.isSafeInteger(policy.maxTimeoutSeconds) && policy.maxTimeoutSeconds > 0,
    "policy.maxTimeoutSeconds must be a positive safe integer");
  return { ...policy, allowedResourceOrigins, maxAmount };
}

function normalizeResource(resource) {
  assert(resource && typeof resource === "object" && typeof resource.url === "string",
    "x402 resource URL is missing");
  let parsed;
  try { parsed = new URL(resource.url); }
  catch { throw new Error("x402 resource URL must be absolute"); }
  assert(parsed.protocol === "https:" && parsed.username === "" && parsed.password === "",
    "x402 resource URL must use HTTPS and must not contain credentials");
  return Object.freeze({
    url: parsed.toString(),
    origin: parsed.origin,
    description: typeof resource.description === "string" ? resource.description : "",
    mimeType: typeof resource.mimeType === "string" ? resource.mimeType : "",
  });
}

/**
 * Evaluate one x402 v2 exact-NEAR offer against explicit local bounds and a
 * caller-supplied policy gate. This is preflight only: it does not create or
 * verify a signed delegate action, contact a facilitator, or settle payment.
 */
export async function authorizeX402NearExactOffer({
  paymentRequired,
  acceptedIndex = 0,
  policy,
  policyGate,
}) {
  assert(paymentRequired && typeof paymentRequired === "object" &&
    paymentRequired.x402Version === 2, "only x402 v2 PaymentRequired objects are supported");
  assert(Array.isArray(paymentRequired.accepts) && paymentRequired.accepts.length > 0,
    "x402 accepts must be a non-empty array");
  assert(Number.isSafeInteger(acceptedIndex) && acceptedIndex >= 0 &&
    acceptedIndex < paymentRequired.accepts.length, "acceptedIndex is out of range");
  assert(typeof policyGate === "function", "a QPC policy gate is required");

  const bounds = validatePolicy(policy);
  const resource = normalizeResource(paymentRequired.resource);
  assert(bounds.allowedResourceOrigins.includes(resource.origin),
    "x402 resource origin is not allowlisted");

  const offered = paymentRequired.accepts[acceptedIndex];
  assert(offered && typeof offered === "object", "selected x402 offer is invalid");
  assert(offered.scheme === "exact", "only the x402 exact scheme is supported");
  assert(NEAR_NETWORKS.has(offered.network) && offered.network === bounds.network,
    "x402 offer network is not allowed by policy");
  assert(isNearAccountId(offered.asset) && bounds.allowedAssets.includes(offered.asset),
    "x402 offer asset is not allowlisted");
  assert(isNearAccountId(offered.payTo) && bounds.allowedRecipients.includes(offered.payTo),
    "x402 offer recipient is not allowlisted");
  assert(Number.isSafeInteger(offered.maxTimeoutSeconds) && offered.maxTimeoutSeconds > 0 &&
    offered.maxTimeoutSeconds <= bounds.maxTimeoutSeconds,
  "x402 offer timeout exceeds the policy bound or is invalid");

  const amount = parsePositiveAtomicAmount(offered.amount, "x402 offer amount");
  assert(amount <= bounds.maxAmount, "x402 offer amount exceeds the policy limit");

  const intent = Object.freeze({
    protocol: "QPC_X402_NEAR_EXACT_PREFLIGHT_V1",
    resourceUrl: resource.url,
    resourceOrigin: resource.origin,
    scheme: offered.scheme,
    network: offered.network,
    asset: offered.asset,
    payTo: offered.payTo,
    amount: offered.amount,
    maxTimeoutSeconds: offered.maxTimeoutSeconds,
  });
  const approved = await policyGate({ phase: "x402_preflight", intent, resource });
  assert(approved === true, "QPC policy rejected the x402 offer");

  return Object.freeze({
    state: "POLICY_APPROVED_LOCAL_ONLY",
    intent,
    warnings: Object.freeze([
      "This result approves only the local offer preflight; no payment payload or signature was created or verified.",
      "No x402 facilitator was contacted and no transaction or settlement was attempted.",
      "The x402 exact-NEAR scheme uses Ed25519 or secp256k1 and does not provide ML-DSA verification, shielded privacy, or a QPC on-chain policy proof.",
      "The signed delegate action nonce, block-height expiry, access-key permission, token balance, and settlement receipt remain unverified.",
    ]),
  });
}
