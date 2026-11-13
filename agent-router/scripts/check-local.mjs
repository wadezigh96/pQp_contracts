#!/usr/bin/env node
// Reproducible local-only validation. Never signs, sends or deploys a transaction.
import { spawnSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
const root = fileURLToPath(new URL('../../', import.meta.url));
const tests = readdirSync(resolve(root, 'agent-router/test')).filter(f => f.endsWith('.test.mjs')).sort().map(f => `agent-router/test/${f}`);
const forge = process.env.FORGE_BIN || 'forge';
const checks = [
  ['JS tests', process.execPath, ['--test', ...tests]],
  ...readdirSync(resolve(root, 'agent-router/src')).filter(f => f.endsWith('.mjs')).sort().map(f => [`JS syntax ${f}`, process.execPath, ['--check', `agent-router/src/${f}`]]),
  ['Simulated demo', process.execPath, ['agent-router/demo/simulate.mjs']],
  ...['near/qpc-near-agent-finance', 'zcash/qpc-zcash-agent', 'solana/qpc-native-executor'].flatMap(crate => [
    [`${crate} fmt`, 'cargo', ['fmt', '--manifest-path', `${crate}/Cargo.toml`, '--', '--check']],
    [`${crate} tests`, 'cargo', ['test', '--manifest-path', `${crate}/Cargo.toml`, '--locked']],
    [`${crate} check`, 'cargo', ['check', '--manifest-path', `${crate}/Cargo.toml`, '--locked', ...(crate.startsWith('near/') ? ['--target', 'wasm32-unknown-unknown'] : [])]],
  ]),
  ['NEAR WASM release', 'cargo', ['build', '--manifest-path', 'near/qpc-near-agent-finance/Cargo.toml', '--locked', '--target', 'wasm32-unknown-unknown', '--release']],
  ['Foundry fmt', forge, ['fmt', '--check']],
  ['Foundry tests', forge, ['test', '--summary']],
  ['Foundry build', forge, ['build']],
  ['Diff whitespace', 'git', ['diff', '--check']],
];
const outcomes = [];
for (const [name, command, args] of checks) {
  console.log(`\n=== ${name}: ${command} ${args.join(' ')} ===`);
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', env: { ...process.env, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS || '1' } });
  const state = result.error?.code === 'ENOENT' ? 'BLOCKED' : result.status === 0 ? 'PASS' : 'FAIL';
  outcomes.push({ name, state, exit: result.status, ...(result.error ? { error: result.error.message } : {}) });
}
console.log('\n=== Local-only verification summary ===');
for (const outcome of outcomes) console.log(`${outcome.state}: ${outcome.name} (exit ${outcome.exit ?? 'not started'})`);
console.log('NOT_RUN: live execution, signing, broadcast, deployment, Solana SBF/validator integration, production audit.');
process.exitCode = outcomes.every(x => x.state === 'PASS') ? 0 : 1;
