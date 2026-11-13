import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { prepareSolanaOwnerApprovedExecution, decodeSolanaExactInstruction } from '../src/solana-exact-execution.mjs';
import { computeNativeRecipientCommitment } from '../src/native-adapters.mjs';
import { computeCrossL1AuthorizationCommitment } from '../src/cross-l1-envelope.mjs';
const rust = JSON.parse(readFileSync(new URL('../../solana/qpc-native-executor/fixtures/codec-v1.json', import.meta.url)));
const NOW = 1800000000;
function fixture() {
  const { programId, owner, agent, recipient, genesisHash, policyAddress, intentAddress } = rust;
  const authorization = {
    schema: 'qpc.cross-l1.authorization', version: 1, intentId: 'local-native-1',
    agentIdentity: { namespace: 'solana', id: agent, keyId: agent },
    source: { networkId: 'solana:devnet', nativeChainId: genesisHash, accountId: agent, adapterId: programId },
    destination: { networkId: 'solana:devnet', nativeChainId: genesisHash, adapterId: programId,
      recipientCommitment: computeNativeRecipientCommitment({ chain: 'solana', nativeChainId: genesisHash, recipient }) },
    action: { type: 'asset-transfer', operation: 'pay' },
    asset: { sourceAssetId: 'solana:native', destinationAssetId: 'solana:native', sourceDecimals: 9, destinationDecimals: 9, mappingId: 'solana-native-identity-v1' },
    amount: { sourceAtomic: '123', destinationAtomic: '123' },
    fees: { feeAssetId: 'solana:native', maxFeeAtomic: '5000', maxSlippageBps: 0 }, nonce: '7',
    replayDomain: { authorityId: programId, scope: 'native-payment' },
    policy: { id: 'policy-1', version: 2, commitment: 'a'.repeat(64) },
    time: { issuedAtUnix: NOW, notBeforeUnix: NOW, expiresAtUnix: NOW + 300 },
    settlement: { evidenceClass: 'solana-owner-attested-native-v1', finalityPolicy: 'solana-finalized', minConfirmations: 2 },
    cancellation: { authorityId: owner, revocationEpoch: 1 },
  };
  return { authorization, recipient, nowUnix: NOW, trustedProfile: { programId, owner, genesisHash }, policyAddress, intentAddress };
}

// Reproduction input for the Rust fixture generator. Explicit opt-in diagnostic
// prints a public fixture commitment only, no network, secrets or signing.
if (process.env.QPC_PRINT_SOLANA_FIXTURE_COMMITMENT === '1') console.log(computeCrossL1AuthorizationCommitment(fixture().authorization));

test('all four instructions byte-match the actual Rust encoder fixture', () => {
  const f = fixture(), p = prepareSolanaOwnerApprovedExecution(f);
  assert.equal(p.authorizationCommitment, computeCrossL1AuthorizationCommitment(f.authorization));
  assert.equal(p.authorizationCommitment, rust.authorizationCommitment);
  for (const kind of ['initialize', 'configure', 'approve', 'execute']) {
    assert.equal(p[kind].dataHex, rust[kind], kind);
    assert.equal(p[kind].programId, rust.programId);
  }
  assert.deepEqual(decodeSolanaExactInstruction(p.initialize.dataHex), { kind: 'Initialize', policy: p.policy });
  assert.deepEqual(decodeSolanaExactInstruction(p.approve.dataHex), { kind: 'Approve', intent: p.intent });
  assert.deepEqual(decodeSolanaExactInstruction(p.configure.dataHex), { kind: 'Configure', version: '2', epoch: '1', active: true });
  assert.deepEqual(decodeSolanaExactInstruction(p.execute.dataHex), { kind: 'Execute', nonce: '7' });
  assert.equal(p.initialize.dataHex.length, 154 * 2); assert.equal(p.approve.dataHex.length, 234 * 2);
  assert.equal(p.ownerAttestationRequired, true); assert.equal(p.pdaAddressesValidated, false);
  assert.equal(p.genesisEnforcedByProgram, false); assert.equal(p.feeBudgetEnforcedByProgram, false);
  assert.equal(p.settlement, 'NOT_IMPLEMENTED');
  assert.deepEqual(p.pdaSeeds.intent, [Buffer.from('intent').toString('hex'), p.approve.dataHex.slice(18, 82), '0700000000000000']);
  assert.equal(p.pdaSeeds.policy[0], Buffer.from('policy').toString('hex'));
  assert.deepEqual(p.execute.accounts, [
    { pubkey: rust.agent, isSigner: true, isWritable: true },
    { pubkey: rust.policyAddress, isSigner: false, isWritable: false },
    { pubkey: rust.intentAddress, isSigner: false, isWritable: true },
    { pubkey: rust.recipient, isSigner: false, isWritable: true },
    { pubkey: '11111111111111111111111111111111', isSigner: false, isWritable: false },
  ]);
  assert.equal(p.initialize.accounts[0].isSigner, true); assert.equal(p.initialize.accounts[0].isWritable, true);
  assert.equal(p.approve.accounts[0].pubkey, rust.owner); assert.equal(p.approve.accounts[2].isWritable, true);
  assert.ok(Object.isFrozen(p.intent) && Object.isFrozen(p.execute.accounts[0]));
  f.authorization.amount.destinationAtomic = '999'; assert.equal(p.intent.amount, '123');
});

const changes = [
  ['mainnet', f => f.authorization.destination.networkId = 'solana:mainnet-beta'],
  ['source chain', f => f.authorization.source.nativeChainId = rust.programId],
  ['destination program', f => f.authorization.destination.adapterId = rust.owner],
  ['source program', f => f.authorization.source.adapterId = rust.owner],
  ['genesis', f => f.trustedProfile.genesisHash = rust.programId],
  ['identity', f => f.authorization.agentIdentity.keyId = rust.owner],
  ['recipient', f => f.recipient = rust.owner],
  ['recipient digest', f => f.authorization.destination.recipientCommitment = 'f'.repeat(64)],
  ['account alias', f => f.intentAddress = f.policyAddress],
  ['self payment', f => f.recipient = rust.agent],
  ['zero key', f => f.policyAddress = '11111111111111111111111111111111'],
  ['invalid base58', f => f.intentAddress = '0'.repeat(44)],
  ['noncanonical key', f => f.intentAddress = `1${f.intentAddress}`],
  ['amount mapping', f => f.authorization.amount.destinationAtomic = '124'],
  ['zero amount', f => { f.authorization.amount.destinationAtomic = '0'; f.authorization.amount.sourceAtomic = '0'; }],
  ['decimals', f => f.authorization.asset.destinationDecimals = 18],
  ['token', f => f.authorization.asset.sourceAssetId = 'solana:token'],
  ['mapping', f => f.authorization.asset.mappingId = 'swapped'],
  ['action', f => f.authorization.action.operation = 'approve'],
  ['owner', f => f.authorization.cancellation.authorityId = rust.agent],
  ['epoch zero', f => f.authorization.cancellation.revocationEpoch = 0],
  ['policy zero', f => f.authorization.policy.commitment = '0'.repeat(64)],
  ['replay', f => f.authorization.replayDomain.authorityId = rust.owner],
  ['replay scope', f => f.authorization.replayDomain.scope = 'other'],
  ['expiry equality', f => f.nowUnix = NOW + 300],
  ['future', f => f.nowUnix = NOW - 1],
  ['not before', f => f.authorization.time.notBeforeUnix = NOW + 1],
  ['fee', f => f.authorization.fees.maxFeeAtomic = '0'],
  ['fee asset', f => f.authorization.fees.feeAssetId = 'solana:token'],
  ['slippage', f => f.authorization.fees.maxSlippageBps = 1],
  ['evidence', f => f.authorization.settlement.evidenceClass = 'generic'],
  ['finality', f => f.authorization.settlement.finalityPolicy = 'optimistic'],
  ['extra profile', f => f.trustedProfile.rpcUrl = 'https://example.invalid'],
  ['nonce overflow', f => f.authorization.nonce = '18446744073709551616'],
  ['amount overflow', f => { f.authorization.amount.sourceAtomic = '18446744073709551616'; f.authorization.amount.destinationAtomic = '18446744073709551616'; }],
  ['fee overflow', f => f.authorization.fees.maxFeeAtomic = '18446744073709551616'],
];
for (const [name, mutate] of changes) test(`reject ${name}`, () => { const f = fixture(); mutate(f); assert.throws(() => prepareSolanaOwnerApprovedExecution(f)); });

test('unsigned builder does not pretend supplied PDAs were checked', () => {
  const f = fixture(); [f.policyAddress, f.intentAddress] = [f.intentAddress, f.policyAddress];
  const p = prepareSolanaOwnerApprovedExecution(f);
  assert.equal(p.pdaAddressesValidated, false); assert.equal(p.intent.policy, f.policyAddress);
  // These wrong supplied addresses will fail the Rust runtime PDA checks.
});
test('u64 maximum remains exact through JS binary codec', () => {
  const f = fixture(), max = '18446744073709551615';
  f.authorization.nonce = max; f.authorization.amount.sourceAtomic = max; f.authorization.amount.destinationAtomic = max; f.authorization.fees.maxFeeAtomic = max;
  const p = prepareSolanaOwnerApprovedExecution(f), i = decodeSolanaExactInstruction(p.approve.dataHex).intent;
  assert.equal(i.amount, max); assert.equal(i.maxFee, max); assert.equal(i.nonce, max);
  assert.equal(p.execute.dataHex, '03ffffffffffffffff');
});
test('all instruction decoders reject truncation, trailing bytes, tag and bad booleans', () => {
  for (const kind of ['initialize', 'configure', 'approve', 'execute']) {
    const hex = rust[kind];
    for (let bytes = 0; bytes < hex.length / 2; bytes++) assert.throws(() => decodeSolanaExactInstruction(hex.slice(0, bytes * 2)));
    assert.throws(() => decodeSolanaExactInstruction(`${hex}00`), /trailing/);
  }
  for (const hex of ['04', '03Ff', '0x030000000000000000', '0', 'zz']) assert.throws(() => decodeSolanaExactInstruction(hex));
  for (const kind of ['initialize', 'configure', 'approve']) assert.throws(() => decodeSolanaExactInstruction(`${rust[kind].slice(0, -2)}02`), /boolean/);
  assert.throws(() => decodeSolanaExactInstruction(`00${'00'.repeat(153)}`), /magic/);
});
test('profile and canonical envelope getters cannot change the prepared values', () => {
  for (const target of ['trustedProfile', 'authorization']) {
    const f = fixture(); let reads = 0;
    Object.defineProperty(f[target], target === 'authorization' ? 'intentId' : 'owner', { enumerable: true, get() { reads++; return rust.owner; } });
    assert.throws(() => prepareSolanaOwnerApprovedExecution(f)); assert.equal(reads, 0);
  }
});
