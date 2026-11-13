const TERMINAL_ROUTE_STATES = new Set(["SUCCESS", "FAILED", "REFUNDED", "INCOMPLETE_DEPOSIT"]);

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function isNonEmptyString(value) {
  return typeof value === "string" && value.length > 0;
}

/**
 * Submit an already-reviewed NEAR dispatch through caller-supplied boundaries.
 * This function never owns keys, selects a solver, or constructs a Zcash tx.
 */
export async function submitNearToZcashDispatch(prepared, {
  signAndSendNear,
  submitRoute,
}) {
  assert(prepared?.state === "PREPARED_LOCAL_ONLY",
    "Zcash dispatch is not in PREPARED_LOCAL_ONLY state");
  assert(typeof signAndSendNear === "function",
    "an explicit NEAR wallet signer is required");
  assert(typeof submitRoute === "function",
    "an explicit Zcash route provider is required");

  const nearResult = await signAndSendNear({
    receiverId: prepared.qpcCall.receiverId,
    methodName: prepared.qpcCall.methodName,
    args: prepared.qpcCall.args,
    gas: prepared.qpcCall.gas,
    deposit: prepared.qpcCall.deposit,
  });
  const nearTxHash = nearResult?.transaction?.hash ??
    nearResult?.transaction_outcome?.id ?? nearResult?.transactionHash;
  assert(isNonEmptyString(nearTxHash), "NEAR wallet did not return a transaction hash");

  const route = await submitRoute({
    protocol: prepared.zcashRequest.protocol,
    request: prepared.zcashRequest,
    nearTxHash,
    policyArgsHash: prepared.policyArgsHash,
  });
  assert(route && isNonEmptyString(route.routeId),
    "Zcash route provider did not return a routeId");

  return Object.freeze({
    state: "ROUTE_SUBMITTED",
    nearTxHash,
    routeId: route.routeId,
    providerState: route.state ?? "PENDING",
    warning: "Route submission is not settlement; verify the Zcash receipt independently.",
  });
}

/**
 * Convert a provider status to SETTLED only after independent receipt checking.
 */
export async function verifyNearToZcashRoute(status, expected, verifyReceipt) {
  assert(status && isNonEmptyString(status.routeId), "route status is invalid");
  assert(isNonEmptyString(status.state), "route status state is missing");
  assert(expected && isNonEmptyString(expected.paymentId) &&
    isNonEmptyString(expected.amountZatoshis) &&
    isNonEmptyString(expected.opaqueRecipientRef),
  "trusted expected payment intent is required");
  if (!TERMINAL_ROUTE_STATES.has(status.state)) return { state: status.state, status };
  if (status.state !== "SUCCESS") return { state: status.state, status };

  // Provider fields are untrusted. Compare them with the independently retained,
  // policy-approved intent before asking the receipt verifier to inspect chain data.
  assert(status.paymentId === expected.paymentId, "provider paymentId does not match approved intent");
  assert(status.amountZatoshis === expected.amountZatoshis, "provider amount does not match approved intent");
  assert(status.opaqueRecipientRef === expected.opaqueRecipientRef,
    "provider recipient reference does not match approved intent");
  assert(typeof verifyReceipt === "function",
    "independent Zcash receipt verification is required before marking SETTLED");
  const verified = await verifyReceipt({
    status,
    expectedPaymentId: expected.paymentId,
    expectedAmountZatoshis: expected.amountZatoshis,
    expectedRecipientRef: expected.opaqueRecipientRef,
  });
  assert(verified === true, "independent Zcash receipt verification failed");
  return Object.freeze({ state: "SETTLED", status });
}

export const QPC_ZCASH_ROUTE_TERMINAL_STATES = Object.freeze([
  "SETTLED", "FAILED", "REFUNDED", "INCOMPLETE_DEPOSIT",
]);
