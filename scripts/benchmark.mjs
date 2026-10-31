import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const windows = process.platform === 'win32';
const candidates = [
  process.env.MARIO_BENCHMARK_PYTHON,
  process.env.VIRTUAL_ENV && resolve(process.env.VIRTUAL_ENV, windows ? 'Scripts/python.exe' : 'bin/python'),
  resolve(root, '.venv', windows ? 'Scripts/python.exe' : 'bin/python'),
  resolve(root, '../../.venv', windows ? 'Scripts/python.exe' : 'bin/python'),
].filter(Boolean);
const python = candidates.find(path => existsSync(path)) ?? (windows ? 'python' : 'python3');
const args = process.argv.slice(2);
const command = args[0] === '--test'
  ? ['-m', 'unittest', 'discover', '-s', 'benchmark/tests', '-v']
  : ['-m', 'benchmark', ...args];
const result = spawnSync(python, command, {
  cwd: root, stdio: 'inherit', env: { ...process.env, PYTHONUTF8: '1' },
});
if (result.error) console.error(`无法运行 Python 3.11+：${result.error.message}`);
process.exitCode = result.status ?? 1;
