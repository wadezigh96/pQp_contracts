import { createHash } from 'node:crypto';
import {
  canonicalizeCrossL1AuthorizationEnvelope,
  computeCrossL1AuthorizationCommitment,
} from './cross-l1-envelope.mjs';

// These adapters prepare data, not transactions. No wallet, signer, RPC or
// production evidence verifier is present. Keep mock validation visibly separate.
const preparedHandles = new WeakSet();
const BASE58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const SYSTEM_PROGRAM = '11111111111111111111111111111111';
const CONFIG = Object.freeze({
  near: { asset: 'near:native', decimals: 24, finality: 'near-final' },
  ethereum: { asset: 'ethereum:native', decimals: 18, finality: 'ethereum-finalized' },
  bsc: { asset: 'bsc:native', decimals: 18, finality: 'bsc-finalized' },
  solana: { asset: 'solana:native', decimals: 9, finality: 'solana-finalized' },
  zcash: { asset: 'zcash:native', decimals: 8, finality: 'zcash-confirmations' },
});
function check(ok, message) { if (!ok) throw new Error(message); }
function snapshotData(value, depth = 0) {
  check(depth <= 12, 'native input nesting exceeds bound');
  if (value === null || typeof value !== 'object') {
    check(['string', 'number', 'boolean'].includes(typeof value) || value === null, 'native input must contain data fields');
    return value;
  }
  const array = Array.isArray(value);
  check(Object.getPrototypeOf(value) === (array ? Array.prototype : Object.prototype) || (!array && Object.getPrototypeOf(value) === null), 'native input must contain plain data fields');
  const descriptors = Object.getOwnPropertyDescriptors(value);
  if (array) delete descriptors.length;
  check(Reflect.ownKeys(value).length === Object.keys(descriptors).length + (array ? 1 : 0) &&
    Object.values(descriptors).every(d => 'value' in d && d.enumerable), 'native input must contain only enumerable data fields');
  if (array) {
    check(Object.keys(descriptors).length === value.length && Object.keys(descriptors).every((key, i) => key === String(i)), 'native arrays must contain dense data fields');
    return Object.values(descriptors).map(d => snapshotData(d.value, depth + 1));
  }
  return Object.fromEntries(Object.entries(descriptors).map(([key, d]) => [key, snapshotData(d.value, depth + 1)]));
}
function exact(value, fields, label) {
  check(value !== null && typeof value === 'object' && !Array.isArray(value), `${label} must be an object`);
  check((Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null) &&
    Reflect.ownKeys(value).length === Object.keys(value).length &&
    Object.values(Object.getOwnPropertyDescriptors(value)).every(d => 'value' in d && d.enumerable), `${label} must contain plain data fields`);
  check(Object.keys(value).sort().join('|') === [...fields].sort().join('|'), `${label} has missing or unexpected fields`);
}
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value !== null && typeof value === 'object') return `{${Object.keys(value).sort().map(k => `${JSON.stringify(k)}:${canonical(value[k])}`).join(',')}}`;
  return JSON.stringify(value);
}
function digest(domain, value) { return createHash('sha256').update(domain).update(canonical(value)).digest('hex'); }
function freeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(freeze); Object.freeze(value); }
  return value;
}
function uint(value, label) { check(Number.isSafeInteger(value) && value >= 0, `${label} must be a safe nonnegative integer`); }
function bytes58(value, bytes, label) {
  check(typeof value === 'string' && value.length > 0 && value.length <= 100, `${label} invalid base58`);
  let n = 0n;
  for (const char of value) { const digit = BASE58.indexOf(char); check(digit >= 0, `${label} invalid base58`); n = n * 58n + BigInt(digit); }
  let length = 0; for (let v = n; v > 0n; v >>= 8n) length++;
  const zeros = value.match(/^1*/)[0].length;
  check(length + zeros === bytes, `${label} must decode to ${bytes} bytes`);
}
function nearAccount(value) {
  check(typeof value === 'string' && value.length >= 2 && value.length <= 64 && /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/.test(value), 'invalid NEAR account');
}
function recipient(chain, value) {
  if (chain === 'near') nearAccount(value);
  else if (chain === 'ethereum' || chain === 'bsc') check(typeof value === 'string' && /^0x[0-9a-f]{40}$/.test(value), 'EVM recipient must be canonical lowercase address');
  else if (chain === 'solana') bytes58(value, 32, 'Solana recipient');
  // Zcash MUST NOT use a prefix or regex as shielded UA validation.
  else check(typeof value === 'string' && value.length > 0, 'invalid recipient');
}
function chainIdentity(chain, networkId, nativeChainId) {
  check(Object.hasOwn(CONFIG, chain), 'unsupported native chain');
  if (chain === 'near' || chain === 'zcash') {
    check(['mainnet', 'testnet'].includes(nativeChainId) && networkId === `${chain}:${nativeChainId}`, 'native chain identity mismatch');
  } else if (chain === 'ethereum') {
    check(['1', '11155111'].includes(nativeChainId) && networkId === `eip155:${nativeChainId}`, 'Ethereum native chain identity mismatch');
  } else if (chain === 'bsc') {
    check(['56', '97'].includes(nativeChainId) && networkId === `eip155:${nativeChainId}`, 'BSC native chain identity mismatch');
  } else {
    check(['solana:mainnet-beta', 'solana:testnet', 'solana:devnet'].includes(networkId), 'unsupported Solana network label');
    bytes58(nativeChainId, 32, 'Solana native genesis hash');
  }
}
function sourceChain(source) {
  if (source.networkId.startsWith('eip155:')) return ['56', '97'].includes(source.nativeChainId) ? 'bsc' : 'ethereum';
  return source.networkId.split(':')[0];
}

/** Commitment to the exact recipient spelling and native chain, not an address proof. */
export function computeNativeRecipientCommitment({ chain, nativeChainId, recipient: address }) {
  check(Object.hasOwn(CONFIG, chain) && typeof nativeChainId === 'string', 'unsupported native recipient chain');
  recipient(chain, address);
  return digest('QPC-NATIVE-RECIPIENT-V1\0', { chain, nativeChainId, recipient: address });
}

/** Local, deterministic preparation only. Authorization/authentication is a separate gate. */
export function prepareNativeTransfer({ authorization, chain, request: inputRequest, nowUnix, trustedProfile: inputProfile } = {}) {
  const request = snapshotData(inputRequest);
  const trustedProfile = snapshotData(inputProfile);
  const envelope = canonicalizeCrossL1AuthorizationEnvelope(authorization);
  check(Object.hasOwn(CONFIG, chain), 'unsupported native chain');
  if (chain === 'zcash') throw new Error('Zcash validated shielded UA parsing unsupported in JavaScript; Rust parser integration required; no transparent fallback');
  const config = CONFIG[chain];
  chainIdentity(chain, envelope.destination.networkId, envelope.destination.nativeChainId);
  const originChain = sourceChain(envelope.source);
  chainIdentity(originChain, envelope.source.networkId, envelope.source.nativeChainId);
  check(originChain !== 'zcash', 'Zcash source shielded address validation unsupported in JavaScript');
  recipient(originChain, envelope.source.accountId);
  // Operator-owned configuration must come from outside the untrusted request.
  exact(trustedProfile, ['sourceNetworkId', 'sourceNativeChainId', 'destinationNetworkId', 'destinationNativeChainId',
    'mappingId', 'sourceAssetId', 'destinationAssetId', 'sourceDecimals', 'destinationDecimals',
    'rateNumerator', 'rateDenominator', ...(chain === 'solana' ? ['expectedGenesisHash'] : [])], 'trusted native profile');
  for (const side of ['source', 'destination']) {
    check(trustedProfile[`${side}NetworkId`] === envelope[side].networkId && trustedProfile[`${side}NativeChainId`] === envelope[side].nativeChainId, 'trusted profile network mismatch');
    check(trustedProfile[`${side}AssetId`] === envelope.asset[`${side}AssetId`] && trustedProfile[`${side}Decimals`] === envelope.asset[`${side}Decimals`], 'trusted profile asset/decimals mismatch');
  }
  check(trustedProfile.mappingId === envelope.asset.mappingId, 'unapproved native asset mapping');
  check(envelope.asset.sourceAssetId === CONFIG[originChain].asset && envelope.asset.sourceDecimals === CONFIG[originChain].decimals, 'source native asset mismatch');
  for (const field of ['rateNumerator', 'rateDenominator']) check(typeof trustedProfile[field] === 'string' && /^[1-9][0-9]{0,38}$/.test(trustedProfile[field]), 'trusted mapping rate invalid');
  check(BigInt(envelope.amount.sourceAtomic) * BigInt(trustedProfile.rateNumerator) === BigInt(envelope.amount.destinationAtomic) * BigInt(trustedProfile.rateDenominator), 'native amounts do not match approved exact mapping rate');
  uint(nowUnix, 'nowUnix');
  check(nowUnix >= envelope.time.notBeforeUnix && nowUnix < envelope.time.expiresAtUnix, 'authorization outside valid time window');
  check(envelope.destination.adapterId === `${chain}-native-v1`, 'destination adapter mismatch');
  check(envelope.action.type === 'asset-transfer' && envelope.action.operation === 'pay', 'only exact native pay transfers supported');
  check(envelope.asset.destinationAssetId === config.asset && envelope.asset.destinationDecimals === config.decimals, 'destination native asset mismatch');
  check(envelope.fees.feeAssetId === config.asset && envelope.fees.maxSlippageBps === 0, 'only destination-native fee budgets and zero slippage supported');
  check(envelope.settlement.evidenceClass === `${chain}-native-v1` && envelope.settlement.finalityPolicy === config.finality, 'native evidence/finality policy mismatch');
  const common = ['nativeChainId', 'recipient', 'amountAtomic', 'maxFeeAtomic'];
  const extra = chain === 'near' ? ['actions'] : chain === 'solana'
    ? ['genesisHash', 'recentBlockhash', 'blockHeight', 'lastValidBlockHeight', 'feePayer', 'signers', 'instructions'] : ['calldata'];
  exact(request, [...common, ...extra], 'native request');
  check(request.nativeChainId === envelope.destination.nativeChainId, 'request native chain ID mismatch');
  recipient(chain, request.recipient);
  check(computeNativeRecipientCommitment({ chain, nativeChainId: request.nativeChainId, recipient: request.recipient }) === envelope.destination.recipientCommitment, 'recipient commitment mismatch');
  check(request.amountAtomic === envelope.amount.destinationAtomic, 'native amount mismatch');
  check(request.maxFeeAtomic === envelope.fees.maxFeeAtomic, 'native fee budget mismatch');
  if (chain === 'near') {
    check(Array.isArray(request.actions) && request.actions.length === 1, 'only one NEAR transfer action supported');
    exact(request.actions[0], ['type', 'deposit'], 'NEAR action');
    check(request.actions[0].type === 'Transfer' && request.actions[0].deposit === request.amountAtomic, 'NEAR transfer action mismatch');
  } else if (chain === 'ethereum' || chain === 'bsc') {
    check(request.calldata === '0x', 'nonempty calldata unsupported for native transfer');
    check(BigInt(request.amountAtomic) < 2n ** 256n, 'EVM value overflow');
  } else {
    check(trustedProfile.expectedGenesisHash === envelope.destination.nativeChainId, 'trusted Solana genesis mismatch');
    check(request.genesisHash === request.nativeChainId, 'Solana genesis mismatch');
    bytes58(request.recentBlockhash, 32, 'Solana recent blockhash');
    bytes58(request.feePayer, 32, 'Solana fee payer');
    uint(request.blockHeight, 'blockHeight'); uint(request.lastValidBlockHeight, 'lastValidBlockHeight');
    check(request.blockHeight < request.lastValidBlockHeight, 'Solana blockhash expired or at expiry height');
    check(BigInt(request.amountAtomic) <= 18446744073709551615n, 'Solana lamports overflow');
    check(Array.isArray(request.signers) && request.signers.length === 1 && request.signers[0] === request.feePayer, 'Solana signer set mismatch');
    check(request.feePayer !== request.recipient, 'Solana self-transfer unsupported');
    check(Array.isArray(request.instructions) && request.instructions.length === 1, 'only one Solana transfer instruction supported');
    const instruction = request.instructions[0];
    exact(instruction, ['programId', 'type', 'lamports', 'accounts'], 'Solana instruction');
    check(instruction.programId === SYSTEM_PROGRAM && instruction.type === 'transfer' && instruction.lamports === request.amountAtomic, 'Solana program/instruction mismatch');
    check(canonical(instruction.accounts) === canonical([
      { pubkey: request.feePayer, isSigner: true, isWritable: true },
      { pubkey: request.recipient, isSigner: false, isWritable: true },
    ]), 'Solana accounts mismatch');
  }
  const result = freeze({ state: 'PREPARED_UNSIGNED', chain, authorization: structuredClone(envelope),
    authorizationCommitment: computeCrossL1AuthorizationCommitment(envelope),
    request: structuredClone(request), requestDigest: digest('QPC-NATIVE-PREPARATION-V1\0', { chain, request }) });
  preparedHandles.add(result);
  return result;
}

/** Mock-fixture consistency checks ONLY. Not an RPC decoder or production verifier. */
export function validateMockNativeEvidence({ prepared, evidence: inputEvidence, seenTransactionIds = new Set() } = {}) {
  const evidence = snapshotData(inputEvidence);
  check(preparedHandles.has(prepared), 'valid local prepared handle required');
  exact(evidence, ['authorizationCommitment', 'requestDigest', 'chain', 'nativeChainId', 'transactionId', 'inclusionReference', 'confirmations', 'finalityStatus', 'canonical', 'success', 'executedRequest', 'observedAtUnix', 'feeAtomic', 'outcomes'], 'mock evidence');
  check(evidence.authorizationCommitment === prepared.authorizationCommitment && evidence.requestDigest === prepared.requestDigest, 'mock evidence commitment mismatch');
  check(evidence.chain === prepared.chain && evidence.nativeChainId === prepared.request.nativeChainId, 'mock evidence chain mismatch');
  for (const field of ['transactionId', 'inclusionReference']) check(typeof evidence[field] === 'string' && /^[A-Za-z0-9._:-]{1,256}$/.test(evidence[field]), `mock ${field} invalid`);
  check(seenTransactionIds instanceof Set && !seenTransactionIds.has(evidence.transactionId), 'duplicate mock transaction');
  uint(evidence.confirmations, 'mock confirmations'); uint(evidence.observedAtUnix, 'mock observation time');
  check(evidence.observedAtUnix >= prepared.authorization.time.issuedAtUnix, 'mock observation predates authorization');
  check(evidence.confirmations >= prepared.authorization.settlement.minConfirmations && evidence.finalityStatus === 'FINALIZED', 'mock finality threshold not met');
  check(typeof evidence.feeAtomic === 'string' && /^(0|[1-9][0-9]{0,38})$/.test(evidence.feeAtomic) && BigInt(evidence.feeAtomic) <= BigInt(prepared.authorization.fees.maxFeeAtomic), 'mock actual fee exceeds authorized native budget or is invalid');
  check(evidence.canonical === true && evidence.success === true, 'mock reorg or unsuccessful execution');
  check(canonical(evidence.executedRequest) === canonical(prepared.request), 'mock native execution substitution');
  check(Array.isArray(evidence.outcomes), 'mock outcomes invalid');
  if (prepared.chain === 'near') {
    check(evidence.outcomes.length > 0, 'mock NEAR receipt outcomes required');
    const ids = new Set();
    for (const outcome of evidence.outcomes) {
      exact(outcome, ['id', 'success', 'pending'], 'mock NEAR outcome');
      check(typeof outcome.id === 'string' && outcome.id.length > 0 && !ids.has(outcome.id), 'duplicate or invalid mock NEAR receipt');
      ids.add(outcome.id); check(outcome.success === true && outcome.pending === false, 'mock NEAR partial/failed outcome');
    }
  } else check(evidence.outcomes.length === 0, 'unexpected mock receipt outcomes');
  return freeze({ state: 'MOCK_VALIDATED_NOT_SETTLED', transactionId: evidence.transactionId, authorizationCommitment: prepared.authorizationCommitment });
}

/** Never turn an injected callback or prepared request into a settlement claim. */
export function settleNativeTransfer() {
  throw new Error('Settlement BLOCKED: real native verifier and durable settlement integration not implemented');
}
