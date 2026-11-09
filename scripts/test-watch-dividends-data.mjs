import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(import.meta.dirname, '..');
const interpreter = process.platform === 'win32' ? 'Scripts/python.exe' : 'bin/python';
const python = [
  process.env.VIRTUAL_ENV && resolve(process.env.VIRTUAL_ENV, interpreter),
].filter(Boolean).find(path => existsSync(path)) ?? (process.platform === 'win32' ? 'python' : 'python3');
const result = spawnSync(python, ['-X', 'utf8', 'scripts/test-watch-dividends-data.py'], {
  cwd: root, stdio: 'inherit', env: { ...process.env, PYTHONUTF8: '1' },
});
if (result.error) console.error(`无法运行分红整理验证：${result.error.message}`);
process.exitCode = result.status ?? 1;
