import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve, delimiter } from 'node:path';
import { pathToFileURL } from 'node:url';
import { cargoTargetDir, root, serverBinary } from './server-artifacts.mjs';

const executable = 'mario-server' + (process.platform === 'win32' ? '.exe' : '');

test('Cargo targets use the checkout root, empty values fall back, and Windows binaries keep .exe', () => {
  for (const env of [{}, { CARGO_TARGET_DIR: '' }, { CARGO_TARGET_DIR: ' \t' }]) {
    assert.equal(cargoTargetDir(env), join(root, 'server/target'));
    assert.equal(cargoTargetDir(env, '.runtime/financial-build'), join(root, '.runtime/financial-build'));
    assert.equal(serverBinary({ env }), join(root, 'server/target/debug', executable));
  }
  const relative = { CARGO_TARGET_DIR: '.runtime/custom target' };
  const absolute = { CARGO_TARGET_DIR: join(tmpdir(), 'custom-target') };
  const padded = { CARGO_TARGET_DIR: ' .runtime/edge-spaces ' };
  for (const env of [relative, absolute, padded]) {
    assert.equal(cargoTargetDir(env), resolve(root, env.CARGO_TARGET_DIR));
    assert.equal(serverBinary({ env, platform: 'win32' }), join(cargoTargetDir(env), 'debug/mario-server.exe'));
    assert.equal(serverBinary({ env, platform: 'linux' }), join(cargoTargetDir(env), 'debug/mario-server'));
  }
});

test('standalone binary overrides win without changing caller-relative paths; blank overrides fall back', () => {
  const target = '.runtime/selected-target';
  for (const binary of ['relative server', join(tmpdir(), 'explicit-server')]) {
    assert.equal(serverBinary({ env: { CARGO_TARGET_DIR: target, MARIO_TEST_SERVER_BIN: binary } }), resolve(binary));
  }
  for (const binary of ['', ' \t']) {
    assert.equal(serverBinary({ env: { CARGO_TARGET_DIR: target, MARIO_TEST_SERVER_BIN: binary } }), join(root, target, 'debug', executable));
  }
});

test('target and derived binary selection remain stable from an unrelated working directory', () => {
  const module = pathToFileURL(join(root, 'scripts/server-artifacts.mjs')).href;
  const script = `const m = await import(${JSON.stringify(module)}); console.log(JSON.stringify([m.cargoTargetDir(), m.serverBinary()]));`;
  const env = { ...process.env, CARGO_TARGET_DIR: '.runtime/from-other-cwd', MARIO_TEST_SERVER_BIN: '' };
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', script], { cwd: tmpdir(), env, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  const target = join(root, '.runtime/from-other-cwd');
  assert.deepEqual(JSON.parse(result.stdout), [target, join(target, 'debug', executable)]);
});

function fixture(t, filename) {
  const temporaryRoot = realpathSync(tmpdir());
  const directory = mkdtempSync(join(temporaryRoot, 'mario-artifacts-'));
  assert.equal(dirname(directory), temporaryRoot);
  t.after(() => {
    assert.equal(dirname(directory), temporaryRoot);
    assert(directory.startsWith(join(temporaryRoot, 'mario-artifacts-')));
    rmSync(directory, { recursive: true, force: true });
  });
  mkdirSync(join(directory, 'scripts'));
  for (const name of ['server-artifacts.mjs', filename]) copyFileSync(join(root, 'scripts', name), join(directory, 'scripts', name));
  return directory;
}

const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
const fakeTools = String.raw`
import assert from 'node:assert/strict';
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';
const [tool, ...args] = process.argv.slice(2);
const root = resolve(import.meta.dirname);
appendFileSync(join(root, 'calls.jsonl'), JSON.stringify({ tool, args, cwd: process.cwd(), target: process.env.CARGO_TARGET_DIR, binary: process.env.MARIO_TEST_SERVER_BIN }) + '\n');
if (tool === 'node' && args[0] === 'scripts/server-artifacts.mjs') {
  if (process.env.FAIL_TARGET_HELPER) process.exit(7);
  const { cargoTargetDir } = await import(pathToFileURL(join(root, args[0])));
  console.log(cargoTargetDir());
}
if (tool === 'cargo' && args[0] === 'build') {
  if (process.env.FAIL_FAKE_BUILD) process.exit(8);
  const target = args.includes('--target-dir') ? args[args.indexOf('--target-dir') + 1] : process.env.CARGO_TARGET_DIR;
  assert(target && resolve(target).startsWith(root + sep));
  const binary = join(target, 'debug/mario-server');
  mkdirSync(dirname(binary), { recursive: true });
  const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
  writeFileSync(binary, '#!/bin/sh\nexec ' + quote(process.execPath) + ' ' + quote(join(root, 'fake-tools.mjs')) + ' server "$@"\n', { mode: 0o700 });
}
`;
function installFakeTools(directory) {
  const bin = join(directory, 'bin');
  mkdirSync(bin);
  const dispatcher = join(directory, 'fake-tools.mjs');
  writeFileSync(dispatcher, fakeTools);
  for (const tool of ['node', 'npm', 'cargo']) {
    writeFileSync(join(bin, tool), `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(dispatcher)} ${quote(tool)} "$@"\n`, { mode: 0o700 });
  }
  return { ...process.env, PATH: bin + delimiter + process.env.PATH, MARIO_TEST_SERVER_BIN: 'inherited-stale-binary' };
}
const calls = directory => readFileSync(join(directory, 'calls.jsonl'), 'utf8').trim().split('\n').map(line => JSON.parse(line));
const unixOnly = { skip: process.platform === 'win32' ? 'POSIX shell orchestration fixture; Windows filename selection is covered above' : false };

test('artifact CLI emits its target when invoked through a real checkout symlink', t => {
  const directory = fixture(t, 'test.sh');
  const alias = join(directory, 'checkout-alias');
  symlinkSync(directory, alias, 'junction');
  const result = spawnSync(process.execPath, [join(alias, 'scripts/server-artifacts.mjs')], {
    cwd: tmpdir(), env: { ...process.env, CARGO_TARGET_DIR: '.runtime/alias target' }, encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout.trim(), join(directory, '.runtime/alias target'));
});

test('full gate binds every stage to its own Cargo target and clears inherited binaries', unixOnly, t => {
  const directory = fixture(t, 'test.sh');
  const env = { ...installFakeTools(directory), CARGO_TARGET_DIR: '.runtime/full-build' };
  const result = spawnSync('bash', [join(directory, 'scripts/test.sh')], { cwd: tmpdir(), env, encoding: 'utf8', timeout: 30000 });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const records = calls(directory);
  const target = join(directory, '.runtime/full-build');
  assert(records.some(call => call.tool === 'cargo' && call.args[0] === 'build'));
  assert(records.some(call => call.tool === 'node' && call.args[0] === 'scripts/postgres-cluster.mjs'));
  assert(records.some(call => call.tool === 'npm' && call.args.join(' ') === 'run test:financial'));
  for (const call of records) {
    assert.equal(call.cwd, directory);
    assert.equal(call.binary, undefined);
    if (!(call.tool === 'node' && call.args[0] === 'scripts/server-artifacts.mjs')) assert.equal(call.target, target);
  }
});

test('full gate normalizes an empty target before Cargo and stops when target resolution fails', unixOnly, t => {
  const directory = fixture(t, 'test.sh');
  const env = { ...installFakeTools(directory), CARGO_TARGET_DIR: '' };
  const result = spawnSync('bash', [join(directory, 'scripts/test.sh')], { cwd: tmpdir(), env, encoding: 'utf8', timeout: 30000 });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.equal(calls(directory).find(call => call.tool === 'cargo').target, join(directory, 'server/target'));
  const before = calls(directory).length;
  const failed = spawnSync('bash', [join(directory, 'scripts/test.sh')], { cwd: tmpdir(), env: { ...env, FAIL_TARGET_HELPER: '1' }, encoding: 'utf8', timeout: 30000 });
  assert.notEqual(failed.status, 0);
  assert.deepEqual(calls(directory).slice(before).map(call => call.tool), ['node']);
});

test('financial migration tests, build and HTTP acceptance share one explicitly bound target', async t => {
  for (const configured of [undefined, '', '.runtime/custom-financial', 'absolute']) {
    await t.test(`configured target: ${configured ?? 'unset'}`, t => {
      const directory = fixture(t, 'test-financial.mjs');
      // Substitute only the disposable-process adapter, so this test needs no Cargo or PostgreSQL.
      writeFileSync(join(directory, 'scripts/postgres-cluster.mjs'), String.raw`
import { appendFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
const root = resolve(import.meta.dirname, '..');
export async function run(command, args, env) {
  appendFileSync(join(root, 'calls.jsonl'), JSON.stringify({ tool: command, args, ...env }) + '\n');
  if (args.some(arg => arg.startsWith('SELECT schemaname'))) return 'mario_auth.users\nmario_market.dividend_histories\nmario_market.dividend_plan_snapshots\nmario_market.watch_snapshots';
  return 'same-restored-checksum';
}
export async function withDisposableCluster(name, callback) {
  await callback({ pg: name => '/mock/' + name, pgEnv: {}, createDatabase: async name => 'postgresql://disposable/' + name, directory: root });
}
`);
      const env = { ...process.env, MARIO_TEST_SERVER_BIN: 'inherited-stale-binary' };
      const supplied = configured === 'absolute' ? join(directory, 'absolute financial') : configured;
      if (supplied === undefined) delete env.CARGO_TARGET_DIR;
      else env.CARGO_TARGET_DIR = supplied;
      const result = spawnSync(process.execPath, [join(directory, 'scripts/test-financial.mjs')], { cwd: tmpdir(), env, encoding: 'utf8', timeout: 30000 });
      assert.equal(result.status, 0, result.stdout + result.stderr);
      const target = resolve(directory, supplied || '.runtime/financial-build');
      const records = calls(directory).filter(call => call.tool === 'cargo' || call.tool === process.execPath);
      assert.equal(records.length, 4);
      assert.deepEqual(records.slice(0, 2).map(call => call.args[0]), ['test', 'build']);
      for (const call of records) {
        assert.equal(call.CARGO_TARGET_DIR, target);
        assert.equal(call.MARIO_TEST_SERVER_BIN, join(target, 'debug', executable));
        if (call.tool === 'cargo') assert.equal(call.args[call.args.indexOf('--target-dir') + 1], target);
      }
    });
  }
});

test('missing backup binary fails before PostgreSQL commands or migration fixtures are created', t => {
  const directory = fixture(t, 'test-backup.mjs');
  const pgBin = join(directory, 'pg');
  mkdirSync(pgBin);
  const marker = join(directory, 'postgres-called');
  writeFileSync(join(pgBin, 'initdb'), `#!/bin/sh\n: > ${quote(marker)}\nexit 1\n`, { mode: 0o700 });
  const binary = join(directory, 'missing-server');
  const env = { ...process.env, MARIO_TEST_SERVER_BIN: binary, MARIO_TEST_PG_BIN: pgBin, TMPDIR: directory, TMP: directory, TEMP: directory };
  const result = spawnSync(process.execPath, [join(root, 'scripts/test-backup.mjs')], { cwd: directory, env, encoding: 'utf8', timeout: 30000 });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /ENOENT/);
  assert(result.stderr.includes(binary));
  assert.equal(existsSync(marker), false);
  assert(!readdirSync(directory).some(name => name.startsWith('mario-migration-')));
});

test('source bridge launches its just-built target, ignores stale test overrides and never launches after build failure', unixOnly, async t => {
  for (const configured of ['', '.runtime/local bridge', 'symlink']) {
    await t.test(`configured target: ${configured || 'empty'}`, t => {
      const directory = fixture(t, 'codex-bridge.mjs');
      const targetSetting = configured === 'symlink' ? '.runtime/symlink bridge' : configured;
      const env = { ...installFakeTools(directory), CARGO_TARGET_DIR: targetSetting };
      const alias = join(directory, 'checkout-alias');
      if (configured === 'symlink') symlinkSync(directory, alias, 'junction');
      const launcher = join(configured === 'symlink' ? alias : directory, 'scripts/codex-bridge.mjs');
      const result = spawnSync(process.execPath, [launcher, '--chatgpt-auth', 'status'], { cwd: tmpdir(), env, encoding: 'utf8', timeout: 30000 });
      assert.equal(result.status, 0, result.stdout + result.stderr);
      const records = calls(directory);
      assert.deepEqual(records.map(call => call.tool), ['cargo', 'server']);
      const target = resolve(directory, targetSetting || 'server/target');
      assert.equal(records[0].args[records[0].args.indexOf('--target-dir') + 1], target);
      assert.deepEqual(records[1].args, ['--chatgpt-auth', 'status']);
      const failed = spawnSync(process.execPath, [launcher, '--chatgpt-auth', 'status'], { cwd: tmpdir(), env: { ...env, FAIL_FAKE_BUILD: '1' }, encoding: 'utf8', timeout: 30000 });
      assert.notEqual(failed.status, 0);
      assert.deepEqual(calls(directory).slice(records.length).map(call => call.tool), ['cargo']);
    });
  }
});
