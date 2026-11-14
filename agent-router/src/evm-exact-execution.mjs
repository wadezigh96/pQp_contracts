import { createHash } from 'node:crypto';
import { canonicalizeCrossL1AuthorizationEnvelope, computeCrossL1AuthorizationCommitment } from './cross-l1-envelope.mjs';
import { computeNativeRecipientCommitment } from './native-adapters.mjs';

const ZERO = `0x${'0'.repeat(40)}`;
const address = value => typeof value === 'string' && /^0x[0-9a-f]{40}$/.test(value) && value !== ZERO;
function check(ok, message) { if (!ok) throw new Error(message); }
const digest = (domain, text) => `0x${createHash('sha256').update(domain).update(text).digest('hex')}`;
function freeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(freeze); Object.freeze(value); }
  return value;
}
function operatorProfile(input) {
  check(input && [Object.prototype, null].includes(Object.getPrototypeOf(input)), 'plain operator profile required');
  const fields = ['chain', 'nativeChainId', 'contractAddress', 'ownerAddress'];
  const descriptors = Object.getOwnPropertyDescriptors(input);
  check(Reflect.ownKeys(input).length === fields.length && fields.every(k => descriptors[k]?.enumerable && 'value' in descriptors[k]), 'exact operator profile data fields required');
  const result = Object.fromEntries(fields.map(k => [k, descriptors[k].value]));
  check((result.chain === 'ethereum' && result.nativeChainId === '11155111') ||
    (result.chain === 'bsc' && result.nativeChainId === '97'), 'only Sepolia/BSC testnet supported');
  check(address(result.contractAddress) && address(result.ownerAddress) && result.contractAddress !== result.ownerAddress, 'invalid operator contract/owner address');
  return result;
}

/** Prepare ONLY data for QPCNativeTestnetExecutor. No RPC, ABI signing or sends.
 * The owner must independently approve the association between the canonical
 * commitment and these typed parameters. The Solidity contract does not parse
 * canonical JSON. A returned preparation is not authorization or settlement.
 */
export function prepareEvmOwnerApprovedExecution({ authorization: input, recipient, nowUnix, trustedProfile } = {}) {
  const p = operatorProfile(trustedProfile);
  const e = canonicalizeCrossL1AuthorizationEnvelope(input);
  const network = `eip155:${p.nativeChainId}`, asset = `${p.chain}:native`;
  check(address(recipient) && recipient !== p.contractAddress, 'invalid canonical recipient address');
  check(address(e.source.accountId) && e.source.accountId !== p.ownerAddress && e.source.accountId !== p.contractAddress, 'invalid agent address');
  check(e.agentIdentity.namespace === 'eip155' && e.agentIdentity.id === e.source.accountId && e.agentIdentity.keyId === e.source.accountId, 'native agent identity mismatch');
  for (const side of ['source', 'destination']) {
    check(e[side].networkId === network && e[side].nativeChainId === p.nativeChainId && e[side].adapterId === p.contractAddress, 'same-chain executor binding mismatch');
    check(e.asset[`${side}AssetId`] === asset && e.asset[`${side}Decimals`] === 18, 'native asset/decimals mismatch');
  }
  check(e.asset.mappingId === `${p.chain}-native-identity-v1` && e.amount.sourceAtomic === e.amount.destinationAtomic, 'exact identity mapping required');
  check(e.action.type === 'asset-transfer' && e.action.operation === 'pay', 'only native payment supported');
  check(e.fees.feeAssetId === asset && e.fees.maxSlippageBps === 0 && BigInt(e.fees.maxFeeAtomic) > 0n, 'invalid all-in native fee budget');
  check(e.replayDomain.authorityId === p.contractAddress && e.replayDomain.scope === 'native-payment', 'executor replay domain mismatch');
  check(e.cancellation.authorityId === p.ownerAddress && e.cancellation.revocationEpoch >= 1, 'owner revocation authority/epoch mismatch');
  check(e.settlement.evidenceClass === 'evm-owner-attested-native-v1' && e.settlement.finalityPolicy === `${p.chain}-finalized` && e.settlement.minConfirmations >= 1, 'unsupported settlement evidence policy');
  check(Number.isSafeInteger(nowUnix) && nowUnix >= e.time.issuedAtUnix && nowUnix >= e.time.notBeforeUnix && nowUnix < e.time.expiresAtUnix, 'authorization outside valid time window');
  check(computeNativeRecipientCommitment({ chain: p.chain, nativeChainId: p.nativeChainId, recipient }) === e.destination.recipientCommitment, 'recipient commitment mismatch');
  const commitment = `0x${computeCrossL1AuthorizationCommitment(e)}`;
  const intent = {
    intentIdHash: digest('QPC-EVM-INTENT-ID-V1\0', e.intentId),
    policyIdHash: digest('QPC-EVM-POLICY-ID-V1\0', e.policy.id),
    policyCommitment: `0x${e.policy.commitment}`,
    agent: e.source.accountId, recipient, asset: ZERO, decimals: 18,
    amount: e.amount.destinationAtomic, chainId: p.nativeChainId, executor: p.contractAddress,
    nonce: e.nonce, policyVersion: String(e.policy.version), revocationEpoch: String(e.cancellation.revocationEpoch),
    issuedAt: String(e.time.issuedAtUnix), notBefore: String(e.time.notBeforeUnix), expiry: String(e.time.expiresAtUnix),
    maxFee: e.fees.maxFeeAtomic,
  };
  return freeze({
    state: 'PREPARED_EVM_OWNER_APPROVAL_UNSIGNED', authorization: e, authorizationCommitment: commitment,
    ownerAttestationRequired: true, feeBudgetEnforcedByContract: false, settlement: 'NOT_IMPLEMENTED',
    configureAgent: { method: 'configureAgent', args: [intent.agent, intent.policyIdHash, intent.policyVersion, intent.revocationEpoch], value: '0' },
    approveIntent: { method: 'approveIntent', args: [commitment, intent], value: '0' },
    execute: { method: 'execute', args: [commitment], to: p.contractAddress, from: intent.agent, value: intent.amount },
  });
}
