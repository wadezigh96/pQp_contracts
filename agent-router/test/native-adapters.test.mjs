import test from 'node:test';
import assert from 'node:assert/strict';
import {
  prepareNativeTransfer, computeNativeRecipientCommitment,
  validateMockNativeEvidence, settleNativeTransfer,
} from '../src/native-adapters.mjs';

const ADDRESS = '0x' + '1'.repeat(40);
const PUBKEY = '11111111111111111111111111111111';
const GENESIS = '4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY';
const NOW = 1800000000;
const configs = {
  near: ['near:testnet', 'testnet', 'near:native', 24, 'bob.testnet', 'near-final'],
  ethereum: ['eip155:1', '1', 'ethereum:native', 18, ADDRESS, 'ethereum-finalized'],
  bsc: ['eip155:56', '56', 'bsc:native', 18, ADDRESS, 'bsc-finalized'],
  solana: ['solana:testnet', GENESIS, 'solana:native', 9, PUBKEY, 'solana-finalized'],
  zcash: ['zcash:testnet', 'testnet', 'zcash:native', 8, 'utest1not-a-valid-address', 'zcash-confirmations'],
};
function fixture(chain = 'ethereum') {
  const [networkId, nativeChainId, asset, decimals, recipient, finalityPolicy] = configs[chain];
  const authorization = {
    schema: 'qpc.cross-l1.authorization', version: 1, intentId: 'native-1',
    agentIdentity: { namespace: 'qpc', id: 'agent-1', keyId: 'key-1' },
    source: { networkId: 'near:testnet', nativeChainId: 'testnet', accountId: 'agent.testnet', adapterId: 'near-native-v1' },
    destination: { networkId, nativeChainId, adapterId: `${chain}-native-v1`, recipientCommitment: computeNativeRecipientCommitment({ chain, nativeChainId, recipient }) },
    action: { type: 'asset-transfer', operation: 'pay' },
    asset: { sourceAssetId: 'near:native', destinationAssetId: asset, sourceDecimals: 24, destinationDecimals: decimals, mappingId: 'native-map-v1' },
    amount: { sourceAtomic: '10', destinationAtomic: '7' },
    fees: { maxFeeAtomic: '2', feeAssetId: asset, maxSlippageBps: 0 }, nonce: '1',
    replayDomain: { authorityId: 'router', scope: 'native' }, policy: { id: 'p1', version: 1, commitment: 'a'.repeat(64) },
    time: { issuedAtUnix: NOW, notBeforeUnix: NOW, expiresAtUnix: NOW + 100 },
    settlement: { evidenceClass: `${chain}-native-v1`, finalityPolicy, minConfirmations: 2 },
    cancellation: { authorityId: 'owner', revocationEpoch: 0 },
  };
  let request = { nativeChainId, recipient, amountAtomic: '7', maxFeeAtomic: '2' };
  if (chain === 'ethereum' || chain === 'bsc') request.calldata = '0x';
  if (chain === 'near') request.actions = [{ type: 'Transfer', deposit: '7' }];
  if (chain === 'solana') request = {
    ...request, genesisHash: GENESIS, recentBlockhash: PUBKEY, blockHeight: 100, lastValidBlockHeight: 150,
    feePayer: 'Vote111111111111111111111111111111111111111',
    signers: ['Vote111111111111111111111111111111111111111'],
    instructions: [{ programId: PUBKEY, type: 'transfer', lamports: '7', accounts: [
      { pubkey: 'Vote111111111111111111111111111111111111111', isSigner: true, isWritable: true },
      { pubkey: recipient, isSigner: false, isWritable: true },
    ] }],
  };
  return { chain, authorization, request, nowUnix: NOW,
    trustedProfile: {
      sourceNetworkId: authorization.source.networkId, sourceNativeChainId: authorization.source.nativeChainId,
      destinationNetworkId: networkId, destinationNativeChainId: nativeChainId,
      mappingId: 'native-map-v1', sourceAssetId: 'near:native', destinationAssetId: asset,
      sourceDecimals: 24, destinationDecimals: decimals, rateNumerator: '7', rateDenominator: '10',
      ...(chain === 'solana' ? { expectedGenesisHash: GENESIS } : {}),
    },
  };
}
function mockEvidence(prepared) {
  return {
    authorizationCommitment: prepared.authorizationCommitment, requestDigest: prepared.requestDigest,
    chain: prepared.chain, nativeChainId: prepared.request.nativeChainId,
    transactionId: 'tx-1', inclusionReference: 'block-1', confirmations: 2,
    finalityStatus: 'FINALIZED', canonical: true, success: true,
    executedRequest: structuredClone(prepared.request), observedAtUnix: NOW + 1, feeAtomic: '1',
    outcomes: prepared.chain === 'near' ? [{ id: 'receipt-1', success: true, pending: false }] : [],
  };
}

test('native preparation rejects accessor requests before reading approved amount', () => {
  const input = fixture();
  Object.defineProperty(input.request, 'amountAtomic', { enumerable: true, get: () => '7' });
  assert.throws(() => prepareNativeTransfer(input), /data fields/);
});
test('native preparation rejects nested accessors and mutable profile getters', () => {
  const input = fixture('near');
  Object.defineProperty(input.request.actions[0], 'deposit', { enumerable: true, get: () => '7' });
  assert.throws(() => prepareNativeTransfer(input), /data fields/);
  const second = fixture();
  Object.defineProperty(second.trustedProfile, 'rateNumerator', { enumerable: true, get: () => '7' });
  assert.throws(() => prepareNativeTransfer(second), /data fields/);
});

for (const chain of ['near', 'ethereum', 'bsc', 'solana']) {
  test(`${chain}: prepares unsigned native request; mock evidence never settles`, () => {
    const p = prepareNativeTransfer(fixture(chain));
    assert.equal(p.state, 'PREPARED_UNSIGNED');
    assert.equal(validateMockNativeEvidence({ prepared: p, evidence: mockEvidence(p) }).state, 'MOCK_VALIDATED_NOT_SETTLED');
    assert.throws(() => settleNativeTransfer({ prepared: p, verify: () => true }), /real native verifier.*not implemented/);
  });
  for (const key of ['recipient', 'amountAtomic', 'nativeChainId', 'maxFeeAtomic']) {
    test(`${chain}: rejects substituted ${key}`, () => {
      const f = fixture(chain); f.request[key] = key === 'recipient' ? 'other' : '999';
      assert.throws(() => prepareNativeTransfer(f));
    });
  }
  test(`${chain}: rejects forged, partial, reorg, invalid and duplicate mock observations`, () => {
    const p = prepareNativeTransfer(fixture(chain));
    for (const mutate of [
      e => { e.authorizationCommitment = 'b'.repeat(64); },
      e => { e.requestDigest = 'b'.repeat(64); },
      e => { e.executedRequest.amountAtomic = '6'; },
      e => { e.confirmations = 1; }, e => { e.canonical = false; },
      e => { e.feeAtomic = '3'; }, e => { e.feeAtomic = '01'; },
      e => { e.finalityStatus = 'SAFE'; }, e => { e.success = false; },
      e => { delete e.inclusionReference; }, e => { e.observedAtUnix = NOW - 1; },
    ]) { const e = mockEvidence(p); mutate(e); assert.throws(() => validateMockNativeEvidence({ prepared: p, evidence: e })); }
    assert.throws(() => validateMockNativeEvidence({ prepared: { ...p }, evidence: mockEvidence(p) }), /prepared handle/);
    assert.throws(() => validateMockNativeEvidence({ prepared: p, evidence: mockEvidence(p), seenTransactionIds: new Set(['tx-1']) }), /duplicate/);
  });
}
test('native chain identity, action, asset, recipient, finality and time are bound', () => {
  for (const mutate of [
    f => { f.authorization.source.nativeChainId = 'mainnet'; },
    f => { f.authorization.source.accountId = 'invalid/account'; },
    f => { f.authorization.asset.sourceDecimals = 18; },
    f => { f.authorization.asset.mappingId = 'unapproved'; },
    f => { f.authorization.amount.sourceAtomic = '11'; },
    f => { delete f.trustedProfile; },
    f => { f.authorization.destination.nativeChainId = '56'; },
    f => { f.authorization.action.operation = 'approve'; },
    f => { f.authorization.asset.destinationAssetId = 'erc20:token'; },
    f => { f.authorization.asset.destinationDecimals = 6; },
    f => { f.authorization.destination.recipientCommitment = '0'.repeat(64); },
    f => { f.authorization.settlement.finalityPolicy = 'safe'; },
    f => { f.nowUnix = NOW + 100; },
    f => { f.request.calldata = '0x1234'; },
    f => { f.request.extra = 'hidden'; },
  ]) { const f = fixture(); mutate(f); assert.throws(() => prepareNativeTransfer(f)); }
});
test('Ethereum and BSC never share chain-id acceptance', () => {
  const eth = fixture('ethereum'); eth.request.nativeChainId = '56'; assert.throws(() => prepareNativeTransfer(eth));
  const bsc = fixture('bsc'); bsc.authorization.destination.nativeChainId = '1'; assert.throws(() => prepareNativeTransfer(bsc));
});
test('NEAR rejects function calls, partial and duplicate receipt outcomes', () => {
  const f = fixture('near'); f.request.actions.push({ type: 'FunctionCall', methodName: 'transfer' });
  assert.throws(() => prepareNativeTransfer(f));
  const p = prepareNativeTransfer(fixture('near'));
  for (const outcomes of [[], [{ id: 'r', success: false, pending: false }], [{ id: 'r', success: true, pending: true }], [{ id: 'r', success: true, pending: false }, { id: 'r', success: true, pending: false }]]) {
    assert.throws(() => validateMockNativeEvidence({ prepared: p, evidence: { ...mockEvidence(p), outcomes } }));
  }
});
test('Solana rejects genesis, program, instructions, account, signer and block HEIGHT substitution', () => {
  for (const mutate of [
    r => { r.genesisHash = PUBKEY; }, r => { r.recentBlockhash = 'bad'; },
    r => { r.instructions[0].programId = r.feePayer; },
    r => { r.instructions.push(structuredClone(r.instructions[0])); },
    r => { r.instructions[0].accounts[1].pubkey = r.feePayer; },
    r => { r.instructions[0].accounts[0].isSigner = false; },
    r => { r.signers.push(PUBKEY); }, r => { r.lastValidBlockHeight = 99; },
    r => { r.instructions[0].lamports = '8'; },
  ]) { const f = fixture('solana'); mutate(f.request); assert.throws(() => prepareNativeTransfer(f)); }
});
test('Solana independently configured genesis must match the authorization and request', () => {
  const f = fixture('solana'); f.trustedProfile.expectedGenesisHash = PUBKEY;
  assert.throws(() => prepareNativeTransfer(f), /trusted Solana genesis/);
});
test('Zcash blocks even plausible shielded UA and never falls back to transparent', () => {
  for (const recipient of ['utest1not-a-valid-address', 'tmTransparentAddress', 'u1plausible']) {
    const f = fixture('zcash'); f.request.recipient = recipient;
    assert.throws(() => prepareNativeTransfer(f), /Zcash.*shielded.*unsupported/);
  }
});
test('mock-only callback harness rejects timeout and invalid RPC-shaped responses', async () => {
  // This harness is deliberately test-local: an injected callback is not a verifier.
  const p = prepareNativeTransfer(fixture());
  async function observe(callback) {
    let timer;
    try {
      const evidence = await Promise.race([callback(), new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('mock timeout')), 10); })]);
      return validateMockNativeEvidence({ prepared: p, evidence });
    } finally { clearTimeout(timer); }
  }
  await assert.rejects(observe(() => new Promise(() => {})), /timeout/);
  for (const response of [null, {}, { result: '0x1' }, true]) await assert.rejects(observe(async () => response));
  assert.equal((await observe(async () => mockEvidence(p))).state, 'MOCK_VALIDATED_NOT_SETTLED');
});
