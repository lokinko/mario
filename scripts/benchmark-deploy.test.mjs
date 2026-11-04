import assert from 'node:assert/strict';
import { test } from 'node:test';
import { join, resolve } from 'node:path';
import { benchDir, benchPython, benchEnvironment, buildDashboard } from './benchmark-deploy.mjs';
import { root } from './start-common.mjs';

test('dashboard uses its deployment interpreter despite inherited root settings', async () => {
  const inherited = {
    PATH: process.env.PATH,
    HOME: '/home/mario',
    VIRTUAL_ENV: '/root/old-venv',
    CONDA_PREFIX: '/root/conda',
    PYTHONHOME: '/root/python',
    PYTHONPATH: '/root/modules',
    UV_PYTHON: '/root/python/bin/python',
    UV_PYTHON_PREFERENCE: 'only-system',
    UV_NO_MANAGED_PYTHON: 'true',
    UV_ACTIVE: 'true',
    UV_PROJECT_ENVIRONMENT: '/root/old-venv',
    UV_PYTHON_INSTALL_DIR: '/root/.local/share/uv/python',
    UV_CACHE_DIR: '/root/.cache/uv',
    MARIO_BENCHMARK_PYTHON: '/root/old-venv/bin/python',
  };
  const original = { ...inherited };
  const calls = [];
  await buildDashboard({ MARIO_BENCH_ENABLED: 'true' }, inherited, async (command, argv, options) => {
    calls.push([command, argv]);
    const env = options.env;
    const runtime = resolve(root, '.runtime/benchmark');
    assert.equal(env.HOME, '/home/mario');
    assert.equal(env.PATH, inherited.PATH);
    assert.equal(env.UV_PROJECT_ENVIRONMENT, join(runtime, '.venv'));
    assert.equal(env.UV_PYTHON_INSTALL_DIR, join(runtime, 'python'));
    assert.equal(env.UV_CACHE_DIR, join(runtime, 'uv-cache'));
    assert.equal(env.MARIO_BENCHMARK_PYTHON, join(runtime, '.venv', process.platform === 'win32' ? 'Scripts/python.exe' : 'bin/python'));
    for (const key of ['VIRTUAL_ENV', 'CONDA_PREFIX', 'PYTHONHOME', 'PYTHONPATH', 'UV_PYTHON', 'UV_PYTHON_PREFERENCE', 'UV_NO_MANAGED_PYTHON', 'UV_ACTIVE'])
      assert.equal(Object.hasOwn(env, key), false, key);
  });
  assert.deepEqual(inherited, original);
  assert.deepEqual(calls, [
    ['uv', ['sync', '--frozen', '--group', 'publish', '--managed-python']],
    [benchPython(), ['-m', 'benchmark.live', 'sync', '--out', benchDir()]],
  ]);
});

test('optional dashboard failures stop subsequent steps and identify the failed stage', async t => {
  for (const [failAt, stage] of ['准备 Python 环境', '同步在线评测快照'].entries()) {
    await t.test(stage, async t => {
      const warnings = [];
      t.mock.method(console, 'warn', message => warnings.push(message));
      let calls = 0;
      await buildDashboard({ MARIO_BENCH_ENABLED: 'true' }, {}, async () => {
        if (calls++ === failAt) throw Error('Permission denied');
      });
      assert.equal(calls, failAt + 1);
      assert.equal(warnings.length, 1);
      assert(warnings[0].includes(`${stage}：Permission denied`));
      assert.match(warnings[0], /继续提供上次生成的版本/);
      assert.match(warnings[0], /\.runtime\/benchmark\/\.venv\/bin\/hf auth login/);
    });
  }
});

test('persisted HF endpoint reaches initial sync and watcher despite a different shell setting', async () => {
  const config = { MARIO_BENCH_ENABLED: 'true', HF_ENDPOINT: 'https://hf-mirror.com' };
  const inherited = { HOME: '/home/mario', HF_ENDPOINT: 'https://huggingface.co' };
  const watcher = benchEnvironment(config, inherited);
  assert.equal(watcher.HF_ENDPOINT, config.HF_ENDPOINT);
  assert.equal(watcher.HOME, inherited.HOME);
  let calls = 0;
  await buildDashboard(config, inherited, async (_command, _argv, { env }) => {
    calls++;
    assert.equal(env.HF_ENDPOINT, watcher.HF_ENDPOINT);
  });
  assert.equal(calls, 2);
  assert.equal(inherited.HF_ENDPOINT, 'https://huggingface.co');
  assert.equal(benchEnvironment({}, { HF_ENDPOINT: config.HF_ENDPOINT }).HF_ENDPOINT, config.HF_ENDPOINT);
  assert.equal(benchEnvironment({}, {}).HF_ENDPOINT, 'https://huggingface.co');
  assert.equal(benchEnvironment({ HF_ENDPOINT: '' }, { HF_ENDPOINT: config.HF_ENDPOINT }).HF_ENDPOINT, 'https://huggingface.co');
});

test('disabled dashboard skips tooling; invalid setting is rejected before execution', async t => {
  t.mock.method(console, 'log', () => {});
  let calls = 0;
  const runner = async () => { calls++; };
  await buildDashboard({}, {}, runner);
  await assert.rejects(buildDashboard({ MARIO_BENCH_ENABLED: 'invalid' }, {}, runner), /MARIO_BENCH_ENABLED/);
  assert.equal(calls, 0);
});
