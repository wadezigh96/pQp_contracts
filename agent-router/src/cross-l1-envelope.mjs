import { createHash } from "node:crypto";

const DOMAIN = "QPC-CROSS-L1-AUTHORIZATION-V1\0";
const EVIDENCE_DOMAIN = "QPC-CROSS-L1-SETTLEMENT-EVIDENCE-V1\0";
const HEX_32 = /^[0-9a-f]{64}$/;
const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,255}$/;
const DECIMAL = /^(0|[1-9][0-9]*)$/;

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function exactObject(value, keys, label) {
  assert(value !== null && typeof value === "object" && !Array.isArray(value),
    \`\${label} must be an object\`);
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  assert(actual.length === expected.length &&
    actual.every((key, index) => key === expected[index]),
  \`\${label} has missing or unexpected field\`);
}

function identifier(value, label) {
  assert(typeof value === "string" && IDENTIFIER.test(value), \`\${label} is invalid\`);
  return value;
}

function decimal(value, label, { positive = false } = {}) {
  assert(typeof value === "string" && DECIMAL.test(value), \`\${label} must be a canonical unsigned decimal string\`);
  if (positive) assert(BigInt(value) > 0n, \`\${label} must be greater than zero\`);
  return value;
}

function safeInteger(value, label, { min = 0 } = {}) {
  assert(Number.isSafeInteger(value) && value >= min, \`\${label} must be a safe integer >= \${min}\`);
  return value;
}

function commitment(value, label) {
  assert(typeof value === "string" && HEX_32.test(value), \`\${label} must be a 32-byte lowercase hex commitment\`);
  return value;
}

function canonicalStringify(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return \`[\${value.map(canonicalStringify).join(",")}]\`;
  return \`{\${Object.keys(value).sort().map(key =>
    \`\${JSON.stringify(key)}:\${canonicalStringify(value[key])}\`).join(",")}}\`;
}

/**
 * Strict QPC v1 envelope validator and canonical projection.
 * This is a commitment format, not an authentication or privacy primitive.
 */
export function canonicalizeCrossL1AuthorizationEnvelope(input) {
  exactObject(input, [
    "schema", "version", "intentId", "agentIdentity", "source", "destination",
    "action", "asset", "amount", "fees", "nonce", "replayDomain", "policy",
    "time", "settlement", "cancellation",
  ], "authorization envelope");

  assert(input.schema === "qpc.cross-l1.authorization" && input.version === 1,
    "unsupported authorization schema or version");

  exactObject(input.agentIdentity, ["namespace", "id", "keyId"], "agentIdentity");
  exactObject(input.source, ["networkId", "accountId", "adapterId"], "source");
  exactObject(input.destination, ["networkId", "adapterId", "recipientCommitment"], "destination");
  exactObject(input.action, ["type", "operation"], "action");
  exactObject(input.asset, ["sourceAssetId", "destinationAssetId", "sourceDecimals", "destinationDecimals", "mappingId"], "asset");
  exactObject(input.amount, ["sourceAtomic", "destinationAtomic"], "amount");
  exactObject(input.fees, ["maxFeeAtomic", "feeAssetId", "maxSlippageBps"], "fees");
  exactObject(input.replayDomain, ["authorityId", "scope"], "replayDomain");
  exactObject(input.policy, ["id", "version", "commitment"], "policy");
  exactObject(input.time, ["issuedAtUnix", "notBeforeUnix", "expiresAtUnix"], "time");
  exactObject(input.settlement, ["evidenceClass", "finalityPolicy", "minConfirmations"], "settlement");
  exactObject(input.cancellation, ["authorityId", "revocationEpoch"], "cancellation");

  const result = {
    schema: input.schema,
    version: input.version,
    intentId: identifier(input.intentId, "intentId"),
    agentIdentity: {
      namespace: identifier(input.agentIdentity.namespace, "agentIdentity.namespace"),
      id: identifier(input.agentIdentity.id, "agentIdentity.id"),
      keyId: identifier(input.agentIdentity.keyId, "agentIdentity.keyId"),
    },
    source: {
      networkId: identifier(input.source.networkId, "source.networkId"),
      accountId: identifier(input.source.accountId, "source.accountId"),
      adapterId: identifier(input.source.adapterId, "source.adapterId"),
    },
    destination: {
      networkId: identifier(input.destination.networkId, "destination.networkId"),
      adapterId: identifier(input.destination.adapterId, "destination.adapterId"),
      recipientCommitment: commitment(input.destination.recipientCommitment, "recipient commitment"),
    },
    action: {
      type: identifier(input.action.type, "action.type"),
      operation: identifier(input.action.operation, "action.operation"),
    },
    asset: {
      sourceAssetId: identifier(input.asset.sourceAssetId, "asset.sourceAssetId"),
      destinationAssetId: identifier(input.asset.destinationAssetId, "asset.destinationAssetId"),
      sourceDecimals: safeInteger(input.asset.sourceDecimals, "asset.sourceDecimals"),
      destinationDecimals: safeInteger(input.asset.destinationDecimals, "asset.destinationDecimals"),
      mappingId: identifier(input.asset.mappingId, "asset.mappingId"),
    },
    amount: {
      sourceAtomic: decimal(input.amount.sourceAtomic, "amount.sourceAtomic", { positive: true }),
      destinationAtomic: decimal(input.amount.destinationAtomic, "amount.destinationAtomic", { positive: true }),
    },
    fees: {
      maxFeeAtomic: decimal(input.fees.maxFeeAtomic, "fees.maxFeeAtomic"),
      feeAssetId: identifier(input.fees.feeAssetId, "fees.feeAssetId"),
      maxSlippageBps: safeInteger(input.fees.maxSlippageBps, "fees.maxSlippageBps"),
    },
    nonce: decimal(input.nonce, "nonce"),
    replayDomain: {
      authorityId: identifier(input.replayDomain.authorityId, "replayDomain.authorityId"),
      scope: identifier(input.replayDomain.scope, "replayDomain.scope"),
    },
    policy: {
      id: identifier(input.policy.id, "policy.id"),
      version: safeInteger(input.policy.version, "policy.version", { min: 1 }),
      commitment: commitment(input.policy.commitment, "policy commitment"),
    },
    time: {
      issuedAtUnix: safeInteger(input.time.issuedAtUnix, "time.issuedAtUnix"),
      notBeforeUnix: safeInteger(input.time.notBeforeUnix, "time.notBeforeUnix"),
      expiresAtUnix: safeInteger(input.time.expiresAtUnix, "time.expiresAtUnix", { min: 1 }),
    },
    settlement: {
      evidenceClass: identifier(input.settlement.evidenceClass, "settlement.evidenceClass"),
      finalityPolicy: identifier(input.settlement.finalityPolicy, "settlement.finalityPolicy"),
      minConfirmations: safeInteger(input.settlement.minConfirmations, "settlement.minConfirmations"),
    },
    cancellation: {
      authorityId: identifier(input.cancellation.authorityId, "cancellation.authorityId"),
      revocationEpoch: safeInteger(input.cancellation.revocationEpoch, "cancellation.revocationEpoch"),
    },
  };

  assert(result.time.notBeforeUnix >= result.time.issuedAtUnix &&
    result.time.expiresAtUnix > result.time.notBeforeUnix,
  "invalid authorization time window");
  assert(result.fees.maxSlippageBps <= 10_000, "fees.maxSlippageBps exceeds 10000");
  assert(result.asset.sourceDecimals <= 255 && result.asset.destinationDecimals <= 255,
    "asset decimals exceed supported bound");
  return Object.freeze(result);
}

export function computeCrossL1AuthorizationCommitment(input) {
  const canonical = canonicalizeCrossL1AuthorizationEnvelope(input);
  return createHash("sha256")
    .update(DOMAIN, "utf8")
    .update(canonicalStringify(canonical), "utf8")
    .digest("hex");
}

/**
 * Authentication is injected by the chain/identity adapter. Replay consumption
 * MUST be backed by durable atomic storage, not this process-local module.
 */
export async function authorizeCrossL1Envelope(input, {
  nowUnix,
  verifyAgentAuthorization,
  consumeReplay,
} = {}) {
  const envelope = canonicalizeCrossL1AuthorizationEnvelope(input);
  safeInteger(nowUnix, "nowUnix");
  assert(nowUnix >= envelope.time.notBeforeUnix, "authorization is not yet valid");
  assert(nowUnix < envelope.time.expiresAtUnix, "authorization has expired");
  assert(typeof verifyAgentAuthorization === "function",
    "agent authorization verifier is required");
  assert(typeof consumeReplay === "function",
    "durable atomic replay consumer is required");

  const authorizationCommitment = computeCrossL1AuthorizationCommitment(envelope);
  const authenticated = await verifyAgentAuthorization({
    envelope,
    authorizationCommitment,
    agentIdentity: envelope.agentIdentity,
    policy: envelope.policy,
  });
  assert(authenticated === true, "agent authorization verification failed");

  const replayDomainKey = [
    envelope.replayDomain.authorityId,
    envelope.replayDomain.scope,
    envelope.source.networkId,
    envelope.source.accountId,
    envelope.agentIdentity.namespace,
    envelope.agentIdentity.id,
  ].join("|");
  const consumed = await consumeReplay({
    replayDomainKey,
    nonce: envelope.nonce,
    intentId: envelope.intentId,
    authorizationCommitment,
  });
  assert(consumed === true, "authorization nonce or intent already consumed");

  return Object.freeze({
    state: "AUTHORIZED",
    authorizationCommitment,
    replayDomainKey,
    intentId: envelope.intentId,
  });
}

export async function verifyCrossL1SettlementEvidence({
  authorization,
  evidence,
  verifyDestinationEvidence,
} = {}) {
  const envelope = canonicalizeCrossL1AuthorizationEnvelope(authorization);
  const expectedCommitment = computeCrossL1AuthorizationCommitment(envelope);

  exactObject(evidence, [
    "schema", "version", "authorizationCommitment", "evidenceClass",
    "destinationNetworkId", "destinationAdapterId", "destinationAssetId",
    "amountAtomic", "recipientCommitment", "transactionId",
    "inclusionReference", "confirmations", "finalityStatus", "proofDigest",
    "observedAtUnix",
  ], "settlement evidence");

  assert(evidence.schema === "qpc.cross-l1.settlement-evidence" && evidence.version === 1,
    "unsupported settlement evidence schema or version");
  assert(evidence.authorizationCommitment === expectedCommitment,
    "settlement authorization commitment mismatch");
  assert(evidence.evidenceClass === envelope.settlement.evidenceClass,
    "settlement evidence class mismatch");
  assert(evidence.destinationNetworkId === envelope.destination.networkId,
    "settlement destination network mismatch");
  assert(evidence.destinationAdapterId === envelope.destination.adapterId,
    "settlement destination adapter mismatch");
  assert(evidence.destinationAssetId === envelope.asset.destinationAssetId,
    "settlement destination asset mismatch");
  assert(decimal(evidence.amountAtomic, "evidence.amountAtomic", { positive: true }) ===
    envelope.amount.destinationAtomic, "settlement amount mismatch");
  assert(commitment(evidence.recipientCommitment, "evidence recipient commitment") ===
    envelope.destination.recipientCommitment, "settlement recipient commitment mismatch");
  identifier(evidence.transactionId, "evidence.transactionId");
  identifier(evidence.inclusionReference, "evidence.inclusionReference");
  commitment(evidence.proofDigest, "evidence proof digest");
  safeInteger(evidence.confirmations, "evidence.confirmations");
  safeInteger(evidence.observedAtUnix, "evidence.observedAtUnix");
  assert(evidence.confirmations >= envelope.settlement.minConfirmations,
    "settlement confirmation threshold not met");
  assert(evidence.finalityStatus === "FINALIZED", "settlement is not finalized");
  assert(evidence.observedAtUnix >= envelope.time.issuedAtUnix,
    "settlement observation predates authorization");

  assert(typeof verifyDestinationEvidence === "function",
    "chain-specific destination evidence verifier is required");
  const verified = await verifyDestinationEvidence({
    authorization: envelope,
    authorizationCommitment: expectedCommitment,
    evidence,
  });
  assert(verified === true, "chain-specific evidence verification failed");

  return Object.freeze({
    state: "EVIDENCE_VERIFIED",
    authorizationCommitment: expectedCommitment,
    transactionId: evidence.transactionId,
    destinationNetworkId: evidence.destinationNetworkId,
    inclusionReference: evidence.inclusionReference,
  });
}
