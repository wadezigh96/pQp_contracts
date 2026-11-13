import { createHash } from "node:crypto";

const HASH_DOMAIN = "QPC-NEAR-ZCASH-POLICY-ARGS-V1\0";
const MAX_IDENTIFIER_BYTES = 128;

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function identifier(value, label) {
  assert(typeof value === "string" && value.length > 0 &&
    Buffer.byteLength(value, "utf8") <= MAX_IDENTIFIER_BYTES &&
    /^[a-zA-Z0-9._:-]+$/.test(value), `${label} is invalid`);
  return value;
}

function decimalAmount(value) {
  assert(typeof value === "string" && /^(0|[1-9][0-9]*)$/.test(value),
    "amountZatoshis must be an unsigned decimal string");
  assert(BigInt(value) > 0n, "amountZatoshis must be greater than zero");
  return value;
}

function canonicalIntent(request) {
  assert(request && typeof request === "object" && !Array.isArray(request),
    "payment request is required");
  const paymentId = identifier(request.paymentId, "paymentId");
  const opaqueRecipientRef = identifier(request.opaqueRecipientRef, "opaqueRecipientRef");
  const amountZatoshis = decimalAmount(request.amountZatoshis);
  assert(Number.isSafeInteger(request.expiresAtUnix) && request.expiresAtUnix > 0,
    "expiresAtUnix must be a positive safe integer");
  return {
    paymentId,
    opaqueRecipientRef,
    amountZatoshis,
    expiresAtUnix: request.expiresAtUnix,
  };
}

/** Hash only the policy-relevant payment intent using a stable field order. */
export function computeNearToZcashPolicyArgsHash(request) {
  const intent = canonicalIntent(request);
  return createHash("sha256")
    .update(HASH_DOMAIN, "utf8")
    .update(JSON.stringify(intent), "utf8")
    .digest("hex");
}

function validNearAccountId(value) {
  return typeof value === "string" && value.length >= 2 && value.length <= 64 &&
    /^[a-z0-9]+([._-][a-z0-9]+)*$/.test(value);
}

/**
 * Prepare a local-only NEAR dispatch. This function does not sign, submit,
 * contact a solver, or create a Zcash transaction.
 */
export function buildNearToZcashShieldedDispatch(input) {
  assert(input && typeof input === "object", "dispatch input is required");
  const intent = canonicalIntent(input);
  assert(Number.isSafeInteger(input.nowUnix) && input.nowUnix >= 0,
    "nowUnix must be a non-negative safe integer");
  assert(intent.expiresAtUnix > input.nowUnix, "payment request is expired");
  assert(validNearAccountId(input.qpcContractId), "qpcContractId is invalid");
  assert(validNearAccountId(input.zcashRouterAccountId), "zcashRouterAccountId is invalid");
  const policyId = identifier(input.policyId, "policyId");
  const requestId = identifier(input.requestId, "requestId");
  const actionId = identifier(input.actionId, "actionId");
  assert(Number.isSafeInteger(input.policyVersion) && input.policyVersion > 0,
    "policyVersion must be a positive safe integer");
  assert(Number.isSafeInteger(input.nonce) && input.nonce >= 0,
    "nonce must be a non-negative safe integer");
  assert(actionId === "pay_zcash_shielded", "only shielded Zcash payment action is permitted");

  const policyArgsHash = computeNearToZcashPolicyArgsHash(input);
  assert(typeof input.policyArgsHash === "string" &&
    /^[0-9a-f]{64}$/.test(input.policyArgsHash) &&
    input.policyArgsHash === policyArgsHash,
  "policyArgsHash does not match canonical payment intent");

  const args = Object.freeze({
    request_id: requestId,
    policy_id: policyId,
    policy_version: input.policyVersion,
    nonce: input.nonce,
    action_id: actionId,
    destination_chain: "zcash:testnet",
    destination_router: input.zcashRouterAccountId,
    payment_id: intent.paymentId,
    amount_zatoshis: intent.amountZatoshis,
    opaque_recipient_ref: intent.opaqueRecipientRef,
    expires_at_unix: intent.expiresAtUnix,
    privacy_policy: "SHIELDED_ONLY",
    policy_args_hash: policyArgsHash,
  });

  const request = Object.freeze({
    payment_id: intent.paymentId,
    amount_zatoshis: intent.amountZatoshis,
    opaque_recipient_ref: intent.opaqueRecipientRef,
    expires_at_unix: intent.expiresAtUnix,
    privacy_policy: "SHIELDED_ONLY",
  });

  return Object.freeze({
    state: "PREPARED_LOCAL_ONLY",
    policyArgsHash,
    qpcCall: Object.freeze({
      receiverId: input.qpcContractId,
      methodName: "dispatch_action",
      args,
      gas: "100000000000000",
      deposit: "0",
    }),
    zcashRequest: Object.freeze({
      protocol: "qpc-zcash-shielded-v1",
      request,
    }),
  });
}

export {
  submitNearToZcashDispatch,
  verifyNearToZcashRoute,
  QPC_ZCASH_ROUTE_TERMINAL_STATES,
} from "./zcash-route.mjs";
