import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { runSimulation, generateDemo } from '../demo/simulate.mjs';
import { renderDashboard } from '../demo/dashboard.mjs';

test('Phase 2 offline simulation deterministically exercises accepted and rejected gates', async () => {
  const first = await runSimulation();
  const second = await runSimulation();
  assert.deepEqual(first, second);
  assert.equal(first.mode, 'SIMULATED');
  assert.equal(first.realTransactions, 0);
  assert.equal(first.realSettlements, 0);
  assert.equal(first.networkRequests, 0);
  assert.equal(first.persistedState, 'AUTHORIZED');
  assert.deepEqual(first.history.map(item => item.state), ['CREATED', 'AUTHORIZED']);
  assert.equal(first.unsignedDispatch.execution, false);
  assert.equal(first.unsignedDispatch.methodName, 'authorize_envelope');
  assert.equal(first.cases.length, 11);
  const cases = Object.fromEntries(first.cases.map(item => [item.id, item]));
  for (const id of ['unauthorized-execution', 'replay', 'expiry', 'rejected-authorization', 'wrong-chain', 'wrong-recipient', 'wrong-amount', 'forged-evidence', 'settlement-blocked']) {
    assert.equal(cases[id].outcome, 'REJECTED', id);
    assert.equal(cases[id].state, 'NO_EXECUTION', id);
  }
  assert.equal(cases['valid-authorization'].state, 'AUTHORIZED');
  assert.equal(cases['matching-mock-evidence'].state, 'MOCK_VALIDATED_NOT_SETTLED');
  assert.equal(first.cases.some(item => item.state === 'SETTLED'), false);
  // The expired envelope must fail before the identity callback; unauthorized
  // execution never invokes it. Valid auth, replay and altered auth do invoke it.
  assert.equal(first.mockVerifierCalls, 3);
});

test('CLI generator emits byte-identical static HTML/JSON and prominently discloses mocks', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qpc-demo-output-'));
  try {
    const a = join(root, 'a'); const b = join(root, 'b');
    await generateDemo(a); await generateDemo(b);
    for (const name of ['index.html', 'report.json']) assert.equal(await readFile(join(a, name), 'utf8'), await readFile(join(b, name), 'utf8'));
    const html = await readFile(join(a, 'index.html'), 'utf8');
    assert.match(html, /SIMULATED/);
    assert.match(html, /No money moves here/);
    assert.match(html, /explicit mock fixtures/);
    assert.match(html, /not cryptographic authentication/);
    assert.match(html, /not a chain verifier/);
    assert.match(html, /MOCK_VALIDATED_NOT_SETTLED/);
    assert.match(html, /report\.json/);
    assert.doesNotMatch(html, /<script|https?:\/\//);
    assert.equal((html.match(/<details /g) ?? []).length, 11);
    const report = JSON.parse(await readFile(join(a, 'report.json'), 'utf8'));
    assert.equal(report.schema, 'qpc.phase2.simulation.v1');
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('dashboard escapes fixture-derived text rather than injecting HTML', async () => {
  const report = await runSimulation();
  report.cases[0].detail = '<script>alert("not executed")</script>';
  const html = renderDashboard(report);
  assert.match(html, /&lt;script&gt;/);
  assert.doesNotMatch(html, /<script>/);
});
