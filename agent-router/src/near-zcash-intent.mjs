import { createHash } from "node:crypto";

const MAX_U64 = 18_446_744_073_709_551_615n;
const MAX_INTENT_TTL_SECONDS = 60 * 60;
const REF_PATTERN = /^ref-[A-Za-z0-9_-]{22,96}$/;
const HASH_PATTERN = /^[0-9a-f]{64}$/i;
const NEAR_ACCOUNT_PATTERN = /^[a-z0-9._-]{2,64}$/;
const QPC_ID_PATTERN = /^[A-Za-z0-9_-]{1,64}$/;
const NEAR_DISPATCH_GAS = "100000000000000";

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function isNearAccountId(value) {
  return typeof value === "string" && NEAR_ACCOUNT_PATTERN.test(value) &&
    !value.startsWith(".") && !value.endsWith(".") && !value.includes("..");
}

function assertQpcId(value, label) {
  assert(typeof value === "string" && QPC_ID_PATTERN.test(value), `${label} is invalid`);
}

function assertSafeInteger(value, label, minimum = 0) {
  assert(Number.isSafeInteger(value) && value >= minimum, `${label} is invalid`);
}

function createCanonicalZcashRequest({
  paymentId, opaqueRecipientRef, amountZatoshis, expiresAtUnix, nowUnix,
  network = "testnet", privacyClass = "shielded",
}) {
  assertQpcId(paymentId, "paymentId");
  assertSafeInteger(nowUnix, "nowUnix");
  assertSafeInteger(expiresAtUnix, "expiresAtUnix", 1);
  assert(expiresAtUnix > nowUnix, "payment intent is expired");
  assert(expiresAtUnix - nowUnix <= MAX_INTENT_TTL_SECONDS,
    "payment intent expiry exceeds the one-hour limit");
  assert(typeof amountZatoshis === "string" && /^[1-9][0-9]*$/.test(amountZatoshis),
    "amountZatoshis must be a positive decimal integer string");
  assert(BigInt(amountZatoshis) <= MAX_U64, "amountZatoshis exceeds the supported u64 range");
  assert(typeof opaqueRecipientRef === "string" && REF_PATTERN.test(opaqueRecipientRef),
    "opaqueRecipientRef must be a high-entropy ref- token, not a Zcash address");
  assert(network === "testnet", "only Zcash Testnet is enabled in this local integration");
  assert(privacyClass === "shielded", "only shielded privacy class is allowed; no transparent fallback");

  // Property order is deliberate: QPC hashes and dispatches these exact UTF-8 bytes.
  const request = {
    protocol: "QPC_NEAR_ZCASH_SHIELDED_V1",
    payment_id: paymentId,
    destination: "zcash",
    network,
    opaque_recipient_ref: opaqueRecipientRef,
    privacy_class: privacyClass,
    amount_zatoshis: amountZatoshis,
    expires_at_unix: expiresAtUnix,
  };
  const bytes = Buffer.from(JSON.stringify(request), "utf8");
  return {
    request,
    bytes,
    hash: createHash("sha256").update(bytes).digest("hex"),
  };
}

/** Compute the exact SHA-256 hash to place in the QPC ActionRule.args_hash. */
export function computeNearToZcashPolicyArgsHash(requestFields) {
  return createCanonicalZcashRequest(requestFields).hash;
}

/**
 * Build a deterministic QPC dispatch payload for one shielded Zcash request.
 * This is a local payload builder only: it does not query NEAR, contact a route
 * provider, sign a transaction, construct a Zcash transaction, or move funds.
 */
export function buildNearToZcashShieldedDispatch({
  qpcContractId,
  policyId,
  policyVersion,
  requestId,
  nonce,
  actionId,
  zcashRouterAccountId,
  paymentId,
  opaqueRecipientRef,
  amountZatoshis,
  expiresAtUnix,
  nowUnix,
  policyArgsHash,
  network = "testnet",
  privacyClass = "shielded",
}) {
  assert(isNearAccountId(qpcContractId), "qpcContractId must be a valid NEAR account ID");
  assert(isNearAccountId(zcashRouterAccountId), "zcashRouterAccountId must be a valid NEAR account ID");
  assertQpcId(policyId, "policyId");
  assertQpcId(requestId, "requestId");
  assertQpcId(actionId, "actionId");
  assertSafeInteger(policyVersion, "policyVersion", 1);
  assertSafeInteger(nonce, "nonce");
  assert(typeof policyArgsHash === "string" && HASH_PATTERN.test(policyArgsHash),
    "policyArgsHash must be the configured 32-byte QPC action argument hash");

  const { request: zcashRequest, bytes: argsBytes, hash: computedPolicyArgsHash } =
    createCanonicalZcashRequest({
      paymentId, opaqueRecipientRef, amountZatoshis, expiresAtUnix, nowUnix,
      network, privacyClass,
    });
  assert(computedPolicyArgsHash === policyArgsHash.toLowerCase(),
    "request does not match the configured QPC policy args hash");

  const actionRequest = {
    policy_id: policyId,
    expected_version: policyVersion,
    request_id: requestId,
    nonce,
    action_id: actionId,
    target: zcashRouterAccountId,
    method_name: "request_shielded_payment",
    args: Object.freeze(Array.from(argsBytes)),
    // No NEAR deposit is attached to this request-intent dispatch.
    attached_deposit: 0,
  };

  return Object.freeze({
    state: "PREPARED_LOCAL_ONLY",
    network,
    privacyClass,
    qpcCall: Object.freeze({
      receiverId: qpcContractId,
      methodName: "dispatch_action",
      args: Object.freeze(actionRequest),
      gas: NEAR_DISPATCH_GAS,
      deposit: "0",
    }),
    zcashRequest: Object.freeze(zcashRequest),
    policyArgsHash: computedPolicyArgsHash,
    warnings: Object.freeze([
      "No on-chain policy was queried; the deployed QPC rule must match policyId, version, target, method, and args hash.",
      "The QPC NEAR contract caps attached NEAR deposit, not ZEC amount; this request is safe only when its exact args hash is pre-approved by policy.",
      "No zcash-router.near deployment, bridge/solver, wallet send capability, receipt verifier, signing, or settlement is configured by this builder.",
      "The opaque recipient reference must be generated with a cryptographically secure random source and resolved only by the private Zcash adapter.",
      "The request metadata and amount are visible to NEAR observers; shielded Zcash does not hide source-chain metadata or route-provider correlation.",
    ]),
  });
}
