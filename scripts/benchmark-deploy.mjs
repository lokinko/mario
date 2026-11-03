import { join, resolve } from 'node:path';
import { root, run } from './start-common.mjs';

export function benchEnabled(value = 'false') {
  if (!['true', 'false'].includes(value)) throw Error('MARIO_BENCH_ENABLED 必须是 true 或 false');
  return value === 'true';
}
export const benchDir = () => resolve(root, '.runtime/benchmark/current');
export const benchPython = () => join(root, '.runtime/benchmark/.venv', process.platform === 'win32' ? 'Scripts/python.exe' : 'bin/python');

// The deployment owns this environment, separately from an administrator's project .venv.
export async function buildDashboard(config, env, runner = run) {
  if (!benchEnabled(config.MARIO_BENCH_ENABLED)) return console.log('未启用 MARIO_BENCH_ENABLED，跳过研究看板');
  const runtime = resolve(root, '.runtime/benchmark');
  const venv = join(runtime, '.venv');
  const python = join(venv, process.platform === 'win32' ? 'Scripts/python.exe' : 'bin/python');
  const isolated = Object.fromEntries(Object.entries(env).filter(([key]) =>
    !/^(VIRTUAL_ENV|CONDA_PREFIX|PYTHONHOME|PYTHONPATH|UV_PYTHON|UV_PYTHON_PREFERENCE|UV_NO_MANAGED_PYTHON|UV_ACTIVE)$/i.test(key)));
  const benchEnv = {
    ...isolated,
    UV_PROJECT_ENVIRONMENT: venv,
    UV_PYTHON_INSTALL_DIR: join(runtime, 'python'),
    UV_CACHE_DIR: join(runtime, 'uv-cache'),
    MARIO_BENCHMARK_PYTHON: python,
  };
  let stage = '准备 Python 环境';
  try {
    await runner('uv', ['sync', '--frozen', '--group', 'publish', '--managed-python'], { env: benchEnv });
    stage = '同步在线评测快照';
    await runner(python, ['-m', 'benchmark.live', 'sync', '--out', benchDir()], { env: benchEnv });
  } catch (error) {
    console.warn(`研究看板未更新（${stage}：${error.message}）；/dashboard 继续提供上次生成的版本，没有则返回“尚未生成”。` +
      '请按该阶段的具体错误排查；私有仓库认证失败时，以服务用户执行 .runtime/benchmark/.venv/bin/hf auth login 保存只读 token，详见 docs/deploy-madeagents.md');
  }
}
