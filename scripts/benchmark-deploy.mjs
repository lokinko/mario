import { join, resolve } from 'node:path';
import { root, run } from './start-common.mjs';

const BENCH_REPOS = ['--repo', 'lokinko/mario_bench', '--labels-repo', 'lokinko/mario_bench_labels'];
export function benchTag(value = '') {
  if (value && !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(value)) throw Error('MARIO_BENCH_TAG 只能包含字母、数字、点、下划线和连字符');
  return value;
}
export const benchDir = tag => resolve(root, '.runtime/benchmark/bench', tag);

// The deployment owns this environment, separately from an administrator's project .venv.
export async function buildDashboard(config, env, runner = run) {
  const tag = benchTag(config.MARIO_BENCH_TAG);
  if (!tag) return console.log('未配置 MARIO_BENCH_TAG，跳过研究看板');
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
  const release = join('.runtime/benchmark', tag);
  let stage = '准备 Python 环境';
  try {
    await runner('uv', ['sync', '--frozen', '--group', 'publish', '--managed-python'], { env: benchEnv });
    stage = '下载并校验 Hugging Face 切片';
    await runner(process.execPath, ['scripts/benchmark.mjs', 'hub-fetch', ...BENCH_REPOS, '--tag', tag, '--out', release], { env: benchEnv });
    stage = '策略评分与生成看板';
    await runner(process.execPath, ['scripts/benchmark.mjs', 'bench-run', '--release', release, '--out', benchDir(tag)], { env: benchEnv });
  } catch (error) {
    console.warn(`研究看板未更新（${stage}：${error.message}）；/dashboard 继续提供上次生成的版本，没有则返回“尚未生成”。` +
      '请按该阶段的具体错误排查；私有仓库认证失败时，以服务用户执行 .runtime/benchmark/.venv/bin/hf auth login 保存只读 token，详见 docs/deploy-madeagents.md');
  }
}
