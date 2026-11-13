import { createHash } from 'node:crypto';
import { canonicalizeCrossL1AuthorizationEnvelope, computeCrossL1AuthorizationCommitment } from './cross-l1-envelope.mjs';
import { computeNativeRecipientCommitment } from './native-adapters.mjs';

export const SOLANA_DEVNET_GENESIS = 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG';
const SYSTEM = '11111111111111111111111111111111';
const ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const MAX_U64 = (1n << 64n) - 1n;
function check(ok, message) { if (!ok) throw new Error(message); }
function base58(bytes) {
  let n = BigInt(`0x${Buffer.from(bytes).toString('hex')}`), text = '';
  while (n) { text = ALPHABET[Number(n % 58n)] + text; n /= 58n; }
  for (const b of bytes) { if (b !== 0) break; text = `1${text}`; }
  return text;
}
function key(text) {
  check(typeof text === 'string' && text.length >= 32 && text.length <= 44, 'invalid base58 key');
  let n = 0n;
  for (const c of text) { const i = ALPHABET.indexOf(c); check(i >= 0, 'invalid base58 key'); n = n * 58n + BigInt(i); }
  const bytes = n === 0n ? Buffer.alloc(0) : Buffer.from(n.toString(16).padStart(Math.ceil(n.toString(16).length / 2) * 2, '0'), 'hex');
  const zeros = text.match(/^1*/)[0].length;
  const result = Buffer.concat([Buffer.alloc(zeros), bytes]);
  check(result.length === 32 && base58(result) === text, 'noncanonical or wrong-sized base58 key');
  return result;
}
function nonzeroKey(text) { const b = key(text); check(b.some(x => x !== 0), 'zero key is not allowed'); return b; }
function u64(value) {
  check(typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value), 'canonical u64 decimal required');
  const n = BigInt(value); check(n <= MAX_U64, 'u64 overflow');
  const b = Buffer.alloc(8); b.writeBigUInt64LE(n); return b;
}
const sha = (domain, text) => createHash('sha256').update(domain).update(text).digest('hex');
const hash = value => { check(typeof value === 'string' && /^[0-9a-f]{64}$/.test(value), '32-byte lowercase hex digest required'); return Buffer.from(value, 'hex'); };
function freeze(v) { if (v && typeof v === 'object') { Object.values(v).forEach(freeze); Object.freeze(v); } return v; }
function profile(input) {
  check(input && [Object.prototype, null].includes(Object.getPrototypeOf(input)), 'plain operator profile required');
  const fields = ['programId', 'owner', 'genesisHash'], d = Object.getOwnPropertyDescriptors(input);
  check(Reflect.ownKeys(input).length === fields.length && fields.every(k => d[k]?.enumerable && 'value' in d[k]), 'exact operator profile data fields required');
  const p = Object.fromEntries(fields.map(k => [k, d[k].value]));
  nonzeroKey(p.programId); nonzeroKey(p.owner);
  check(p.programId !== p.owner && p.genesisHash === SOLANA_DEVNET_GENESIS, 'devnet operator program/owner/genesis mismatch');
  return p;
}
function policyBytes(p) {
  return Buffer.concat([Buffer.from('QPCPOL01'), key(p.owner), key(p.agent), key(p.genesis), hash(p.policyId), u64(p.version), u64(p.epoch), Buffer.from([p.active ? 1 : 0])]);
}
function intentBytes(i) {
  return Buffer.concat([Buffer.from('QPCINT01'), key(i.policy), hash(i.authorizationCommitment), hash(i.intentId), hash(i.policyCommitment), key(i.recipient), ...['nonce', 'amount', 'version', 'epoch', 'issuedAt', 'notBefore', 'expiresAt', 'maxFee'].map(k => u64(i[k])), Buffer.from([i.executed ? 1 : 0])]);
}

/** Strict decoder of the Rust Instruction wire ABI, not proof of authenticity,
 * PDA validity, native execution, current policy, cluster identity or settlement.
 * Digests are hex without 0x; u64 fields are decimal strings (never JS numbers).
 */
export function decodeSolanaExactInstruction(dataHex) {
  check(typeof dataHex === 'string' && /^(?:[0-9a-f]{2})+$/.test(dataHex), 'canonical instruction hex required');
  const b = Buffer.from(dataHex, 'hex'); let offset = 0;
  const take = n => { check(offset + n <= b.length, 'truncated instruction'); const v = b.subarray(offset, offset + n); offset += n; return v; };
  const byte = () => take(1)[0], int = () => take(8).readBigUInt64LE().toString(), pubkey = () => base58(take(32)), digest = () => take(32).toString('hex');
  const bool = () => { const v = byte(); check(v <= 1, 'invalid boolean'); return v === 1; };
  let result;
  switch (byte()) {
    case 0:
      check(take(8).toString() === 'QPCPOL01', 'invalid policy magic');
      result = { kind: 'Initialize', policy: { owner: pubkey(), agent: pubkey(), genesis: pubkey(), policyId: digest(), version: int(), epoch: int(), active: bool() } }; break;
    case 1: result = { kind: 'Configure', version: int(), epoch: int(), active: bool() }; break;
    case 2:
      check(take(8).toString() === 'QPCINT01', 'invalid intent magic');
      result = { kind: 'Approve', intent: { policy: pubkey(), authorizationCommitment: digest(), intentId: digest(), policyCommitment: digest(), recipient: pubkey(), nonce: int(), amount: int(), version: int(), epoch: int(), issuedAt: int(), notBefore: int(), expiresAt: int(), maxFee: int(), executed: bool() } }; break;
    case 3: result = { kind: 'Execute', nonce: int() }; break;
    default: throw new Error('unknown instruction tag');
  }
  check(offset === b.length, 'trailing instruction bytes'); return freeze(result);
}

/** UNSIGNED data ONLY for the local qpc-native-executor Rust ABI. No signing,
 * transaction construction, RPC, transport or deployed-program claims.
 * Owner explicitly attests canonical commitment -> typed parameters; program
 * does not parse JSON. Supply policy/intent addresses from a trusted PDA derivation.
 * This zero-dependency builder does NOT derive/check PDAs; it supplies exact seeds
 * and runtime checks them. Initialize allocates the policy via signed System CPI;
 * Approve allocates the immutable intent. Configure is NOT an automatic prerequisite:
 * its epoch must strictly advance an existing policy (and invalidates older intents).
 * Genesis/maxFee are metadata only. Generic Solana settlement remains NOT_IMPLEMENTED.
 */
export function prepareSolanaOwnerApprovedExecution({ authorization: input, recipient, nowUnix, trustedProfile, policyAddress, intentAddress } = {}) {
  const p = profile(trustedProfile), e = canonicalizeCrossL1AuthorizationEnvelope(input);
  const agent = e.source.accountId;
  [agent, recipient, policyAddress, intentAddress].forEach(nonzeroKey);
  check(new Set([p.programId, p.owner, agent, recipient, policyAddress, intentAddress]).size === 6, 'execution accounts must be distinct');
  check(e.agentIdentity.namespace === 'solana' && e.agentIdentity.id === agent && e.agentIdentity.keyId === agent, 'native agent identity mismatch');
  for (const side of ['source', 'destination']) {
    check(e[side].networkId === 'solana:devnet' && e[side].nativeChainId === p.genesisHash && e[side].adapterId === p.programId, 'same-chain program/genesis binding mismatch');
    check(e.asset[`${side}AssetId`] === 'solana:native' && e.asset[`${side}Decimals`] === 9, 'native asset/decimals mismatch');
  }
  check(e.asset.mappingId === 'solana-native-identity-v1' && e.amount.sourceAtomic === e.amount.destinationAtomic, 'exact native identity mapping required');
  check(e.action.type === 'asset-transfer' && e.action.operation === 'pay', 'only native payment supported');
  check(e.fees.feeAssetId === 'solana:native' && e.fees.maxSlippageBps === 0 && BigInt(e.fees.maxFeeAtomic) > 0n, 'invalid all-in native fee budget');
  check(e.replayDomain.authorityId === p.programId && e.replayDomain.scope === 'native-payment', 'program replay domain mismatch');
  check(e.cancellation.authorityId === p.owner && e.cancellation.revocationEpoch >= 1, 'owner revocation authority/epoch mismatch');
  check(e.policy.commitment !== '0'.repeat(64), 'nonzero policy commitment required');
  check(e.settlement.evidenceClass === 'solana-owner-attested-native-v1' && e.settlement.finalityPolicy === 'solana-finalized' && e.settlement.minConfirmations >= 1, 'unsupported settlement evidence policy');
  check(Number.isSafeInteger(nowUnix) && nowUnix >= e.time.issuedAtUnix && nowUnix >= e.time.notBeforeUnix && nowUnix < e.time.expiresAtUnix, 'authorization outside valid time window');
  check(computeNativeRecipientCommitment({ chain: 'solana', nativeChainId: p.genesisHash, recipient }) === e.destination.recipientCommitment, 'recipient commitment mismatch');
  const authorizationCommitment = computeCrossL1AuthorizationCommitment(e);
  const policy = { owner: p.owner, agent, genesis: p.genesisHash, policyId: sha('QPC-SOLANA-POLICY-ID-V1\0', e.policy.id), version: String(e.policy.version), epoch: String(e.cancellation.revocationEpoch), active: true };
  const intent = { policy: policyAddress, authorizationCommitment, intentId: sha('QPC-SOLANA-INTENT-ID-V1\0', e.intentId), policyCommitment: e.policy.commitment, recipient, nonce: e.nonce, amount: e.amount.destinationAtomic, version: policy.version, epoch: policy.epoch, issuedAt: String(e.time.issuedAtUnix), notBefore: String(e.time.notBeforeUnix), expiresAt: String(e.time.expiresAtUnix), maxFee: e.fees.maxFeeAtomic, executed: false };
  check(BigInt(intent.amount) > 0n, 'positive amount required');
  const meta = (pubkey, isSigner = false, isWritable = false) => ({ pubkey, isSigner, isWritable });
  const instruction = (accounts, data) => ({ programId: p.programId, accounts, dataHex: data.toString('hex') });
  return freeze({
    state: 'PREPARED_SOLANA_OWNER_APPROVAL_UNSIGNED', authorization: e, authorizationCommitment, policyAddress, intentAddress, policy, intent,
    ownerAttestationRequired: true, pdaAddressesValidated: false, genesisEnforcedByProgram: false, feeBudgetEnforcedByProgram: false, settlement: 'NOT_IMPLEMENTED',
    pdaSeeds: { policy: [Buffer.from('policy').toString('hex'), key(p.owner).toString('hex'), key(agent).toString('hex')], intent: [Buffer.from('intent').toString('hex'), key(policyAddress).toString('hex'), u64(intent.nonce).toString('hex')] },
    initialize: instruction([meta(p.owner, true, true), meta(policyAddress, false, true), meta(SYSTEM)], Buffer.concat([Buffer.from([0]), policyBytes(policy)])),
    configure: instruction([meta(p.owner, true), meta(policyAddress, false, true)], Buffer.concat([Buffer.from([1]), u64(policy.version), u64(policy.epoch), Buffer.from([1])])),
    approve: instruction([meta(p.owner, true, true), meta(policyAddress), meta(intentAddress, false, true), meta(SYSTEM)], Buffer.concat([Buffer.from([2]), intentBytes(intent)])),
    execute: instruction([meta(agent, true, true), meta(policyAddress), meta(intentAddress, false, true), meta(recipient, false, true), meta(SYSTEM)], Buffer.concat([Buffer.from([3]), u64(intent.nonce)])),
  });
}
