import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { createConfig, defaults, root } from './start-common.mjs';

const bash = process.env.MARIO_TEST_BASH || 'bash';
const script = readFileSync(join(root, 'deploy.sh'), 'utf8');
const configuration = script.split('# =================== 配置区结束 ===================')[0];
const sources = {
  npm_config_registry: 'https://registry.npmmirror.com',
  NODE_DIST_URL: 'https://npmmirror.com/mirrors/node/latest-v22.x',
  RUSTUP_INIT_URL: 'https://rsproxy.cn/rustup-init.sh',
  RUSTUP_DIST_SERVER: 'https://rsproxy.cn',
  RUSTUP_UPDATE_ROOT: 'https://rsproxy.cn/rustup',
  MARIO_DEPLOY_CARGO_REGISTRY: 'sparse+https://rsproxy.cn/index/',
};
const cleanEnv = Object.fromEntries(Object.entries(process.env).filter(([key]) =>
  !Object.hasOwn(sources, key) && !/^(MARIO_|NPM_CONFIG_REGISTRY$)/i.test(key)));
function configured(overrides = {}) {
  const result = spawnSync(bash, ['-euc', `${configuration}\nexec "$DEPLOY_TEST_NODE" -e 'console.log(JSON.stringify(process.env))'`], {
    env: { ...cleanEnv, ...overrides, DEPLOY_TEST_NODE: process.execPath }, encoding: 'utf8', timeout: 15000,
  });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  return JSON.parse(result.stdout);
}

test('deployment exports domestic defaults and respects source overrides and empty Cargo registry', () => {
  const env = configured();
  for (const [key, value] of Object.entries(sources)) assert.equal(env[key], value);
  const overrides = Object.fromEntries(Object.keys(sources).map(key => [key, `https://mirror.example.test/${key}`]));
  overrides.MARIO_DEPLOY_CARGO_REGISTRY = '';
  const custom = configured(overrides);
  for (const [key, value] of Object.entries(overrides)) assert.equal(custom[key], value);
  assert.equal(configured({ NPM_CONFIG_REGISTRY: 'https://registry.npmjs.org' }).npm_config_registry, 'https://registry.npmjs.org');
});

// Deployment targets Linux; shell stubs prevent downloads, builds, sudo and service changes.
test('automatic sudo escalation preserves source configuration', { skip: process.platform === 'win32' }, () => {
  const fixture = mkdtempSync(join(tmpdir(), 'mario-sources-sudo-'));
  try {
    mkdirSync(join(fixture, 'bin'));
    copyFileSync(join(root, 'deploy.sh'), join(fixture, 'deploy.sh'));
    writeFileSync(join(fixture, 'bin/uname'), '#!/bin/sh\necho Linux\n', { mode: 0o755 });
    writeFileSync(join(fixture, 'bin/sudo'), '#!/bin/sh\nexec "$DEPLOY_TEST_NODE" -e \'console.log(JSON.stringify(process.argv.slice(1)))\' -- "$@"\n', { mode: 0o755 });
    const env = configured({ MARIO_DEPLOY_CARGO_REGISTRY: '', RUSTUP_DIST_SERVER: 'https://rust.example.test' });
    env.PATH = `${join(fixture, 'bin')}:${env.PATH}`;
    env.DEPLOY_TEST_NODE = process.execPath;
    const rootTest = process.getuid?.() === 0;
    // Avoid a real root deployment even when tests run as root in Linux CI.
    const command = rootTest ? 'runuser' : bash;
    const argv = rootTest ? ['-u', 'nobody', '--', bash, join(fixture, 'deploy.sh')] : [join(fixture, 'deploy.sh')];
    if (rootTest) chmodSync(fixture, 0o755);
    const result = spawnSync(command, argv, { env, encoding: 'utf8', timeout: 15000 });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    const forwarded = JSON.parse(result.stdout);
    assert.equal(forwarded[0], 'env');
    for (const key of Object.keys(sources)) assert(forwarded.includes(`${key}=${env[key]}`), key);
  } finally { rmSync(fixture, { recursive: true, force: true }); }
});

test('deployment build passes npm/Rustup sources and applies Cargo replacement only when requested', { skip: process.platform === 'win32' }, () => {
  const fixture = mkdtempSync(join(tmpdir(), 'mario-sources-build-'));
  try {
    for (const dir of ['scripts', 'bin']) mkdirSync(join(fixture, dir));
    for (const name of ['host-runtime.mjs', 'start-common.mjs', 'benchmark-deploy.mjs'])
      copyFileSync(join(root, 'scripts', name), join(fixture, 'scripts', name));
    createConfig(join(fixture, '.env'), { ...defaults(), MARIO_BENCH_ENABLED: 'false' });
    const stub = join(fixture, 'stub.mjs');
    writeFileSync(stub, `console.log(JSON.stringify({ command: process.argv[2].split('/').pop(), argv: process.argv.slice(3), env: Object.fromEntries(Object.entries(process.env).filter(([key]) => ['npm_config_registry', 'RUSTUP_DIST_SERVER', 'RUSTUP_UPDATE_ROOT'].includes(key))) }));\nif (process.env.DEPLOY_TEST_FAIL) process.exit(7);\n`);
    for (const name of ['npm', 'cargo']) writeFileSync(join(fixture, 'bin', name), '#!/bin/sh\nexec "$DEPLOY_TEST_NODE" "$DEPLOY_TEST_STUB" "$0" "$@"\n', { mode: 0o755 });
    function build(overrides = {}, expected = 0) {
      const env = configured(overrides);
      const result = spawnSync(process.execPath, [join(fixture, 'scripts/host-runtime.mjs'), '--build'], {
        env: { ...env, PATH: `${join(fixture, 'bin')}:${env.PATH}`, DEPLOY_TEST_NODE: process.execPath, DEPLOY_TEST_STUB: stub },
        encoding: 'utf8', timeout: 15000,
      });
      assert.equal(result.status, expected, result.stdout + result.stderr);
      return result.stdout.split('\n').filter(line => line.startsWith('{')).map(line => JSON.parse(line));
    }
    for (const registry of [sources.MARIO_DEPLOY_CARGO_REGISTRY, 'sparse+https://cargo.example.test/index/', '']) {
      const calls = build({ MARIO_DEPLOY_CARGO_REGISTRY: registry });
      assert.deepEqual(calls.map(call => call.command), ['npm', 'npm', 'cargo']);
      assert.deepEqual(calls[0].argv, ['ci', '--prefix', 'client']);
      for (const call of calls) {
        for (const key of ['npm_config_registry', 'RUSTUP_DIST_SERVER', 'RUSTUP_UPDATE_ROOT']) assert.equal(call.env[key], sources[key]);
      }
      const argv = calls[2].argv;
      assert.deepEqual(argv.slice(-5), ['build', '--release', '--locked', '--manifest-path', 'server/Cargo.toml']);
      assert.deepEqual(argv.slice(0, -5), registry ? [
        '--config', 'source.crates-io.replace-with="mario-deploy"',
        '--config', `source.mario-deploy.registry=${JSON.stringify(registry)}`,
      ] : []);
    }
    assert.equal(build({ DEPLOY_TEST_FAIL: 'true' }, 1).length, 1, 'npm failure must stop before Cargo');
  } finally { rmSync(fixture, { recursive: true, force: true }); }
});
