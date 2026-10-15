// Local only: build the same audited worker and let Codex own its login state.
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const site = process.argv[2] || 'https://www.madeagents.ai/mario/';
const url = new URL(site);
if ((url.protocol !== 'https:' && !(url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname))) || url.username || url.password || url.search || url.hash) {
  throw Error('请输入 HTTPS Mario 网页地址；仅本机测试允许 HTTP。');
}
function run(command, args) {
  return new Promise((done, reject) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit', shell: false, windowsHide: true });
    child.once('error', reject);
    child.once('exit', code => code === 0 ? done() : reject(Error(`桥接进程退出 (${code})`)));
  });
}
try {
  await run('cargo', ['build', '--manifest-path', 'server/Cargo.toml', '--bin', 'mario-server']);
  await run(resolve(root, `server/target/debug/mario-server${process.platform === 'win32' ? '.exe' : ''}`), ['--codex-bridge', site]);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
