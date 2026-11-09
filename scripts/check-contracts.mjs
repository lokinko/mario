import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createRequire } from 'node:module';
import { run } from './postgres-cluster.mjs';

const root = resolve(import.meta.dirname, '..');
const ts = createRequire(resolve(root, 'client/package.json'))('typescript');
const paths = new Set(['/api/future/preview']);
for (const name of readdirSync(resolve(root, 'server/src/http')).filter(name => name.endsWith('.rs'))) {
  const source = readFileSync(resolve(root, 'server/src/http', name), 'utf8');
  for (const match of source.matchAll(/"(\/api\/[^"\s]+)"/g)) paths.add(match[1].replace(/\{[^}]+\}/g, 'contract-id'));
}
const output = await run('cargo', ['test', '--manifest-path', 'server/Cargo.toml', '--lib', 'transport_contract_fixture', '--', '--ignored', '--nocapture'], { MARIO_CONTRACT_PATHS: JSON.stringify([...paths]) }, true);
const match = output.match(/MARIO_CONTRACT:(\{[^\r\n]+\})/);
assert(match, 'Rust contract fixture did not execute');
const fixture = JSON.parse(match[1]);

// Type-check real Rust JSON in memory, including excess fields and decimal string/null types.
const filename = resolve(root, 'client/transport-contract.generated.ts');
const source = `/// <reference types="vite/client" />
import type { HoldingAmounts, HoldingTradeInput, HoldingTradeUpdateInput, FxBook, AnalysisEvent, ModelConfig } from './src/types';
import type { CodexBridgeStatus } from './src/api/settings';
import type { WatchRules } from './src/api/watch';
const fixture = ${JSON.stringify({ amounts: fixture.amounts, rules: fixture.rules, trades:fixture.trades, tradeUpdates:fixture.tradeUpdates, fx:fixture.fx, events:fixture.events, configs:fixture.configs, bridges:fixture.bridges })} satisfies { amounts: HoldingAmounts[]; rules: WatchRules[]; trades: HoldingTradeInput[]; tradeUpdates:HoldingTradeUpdateInput[]; fx:FxBook; events:AnalysisEvent[]; configs:ModelConfig[]; bridges:CodexBridgeStatus[] };
void fixture;`;
const options = { strict: true, noEmit: true, skipLibCheck: true, target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler, types: [], lib: ['lib.es2022.d.ts', 'lib.dom.d.ts', 'lib.dom.iterable.d.ts'] };
const host = ts.createCompilerHost(options);
const original = host.getSourceFile.bind(host);
host.getSourceFile = (name, language, ...args) => resolve(name) === filename ? ts.createSourceFile(name, source, language, true) : original(name, language, ...args);
const program = ts.createProgram([filename], options, host);
const diagnostics = ts.getPreEmitDiagnostics(program);
assert.equal(diagnostics.length, 0, ts.formatDiagnosticsWithColorAndContext(diagnostics, { getCanonicalFileName: name => name, getCurrentDirectory: () => root, getNewLine: () => '\n' }));

const policySource = readFileSync(resolve(root, 'client/src/lib/requestPolicy.ts'), 'utf8');
const policyJs = ts.transpileModule(policySource, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { isDataWrite } = await import(`data:text/javascript;base64,${Buffer.from(policyJs).toString('base64')}`);
for (const { path, method, requiresRevision } of fixture.policies)
  assert.equal(isDataWrite(path.slice(4), method), requiresRevision, `${method} ${path}: request policies disagree`);
assert.equal(fixture.amounts[0].marketValue, '99999999999999.99');
assert.equal(fixture.amounts[1].quantity, null);
console.log(`PASS contracts: Rust amounts, ${fixture.trades.length} trades, ${fixture.tradeUpdates.length} corrections, FX, ${fixture.events.length} stream events, ${fixture.rules.length} observation rules and ${fixture.policies.length} request policies agree with TypeScript`);
