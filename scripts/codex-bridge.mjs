// Local only: Codex owns its login, or Mario owns an official ChatGPT OAuth grant.
// Neither path sends OpenAI credentials to the Mario site.
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

export function workerArguments(input) {
  const args = [...input];
  const mode = args[0]?.startsWith('--') ? args.shift() : '--codex-bridge';
  if (mode === '--chatgpt-auth') {
    const [action, ...options] = args;
    const valid = (action === 'login' && (options.length === 0 || (options.length === 1 && options[0] === '--new') || (options.length === 2 && options[0] === '--account' && options[1]))) ||
      (action === 'status' && options.length === 0) ||
      (action === 'select' && options.length === 1 && options[0]) ||
      (action === 'logout' && options.length <= 1);
    if (!valid) throw Error('用法：npm run chatgpt:auth -- login [--new | --account <注册 ID>] | status | select <注册 ID> | logout [注册 ID]');
    return [mode, ...args];
  }
  if (!['--codex-bridge', '--chatgpt-bridge'].includes(mode) || args.length > 1) throw Error('桥接只接受一个 Mario 网页地址。');
  const site = args[0] || 'https://www.madeagents.ai/mario/';
  const url = new URL(site);
  if ((url.protocol !== 'https:' && !(url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname))) || url.username || url.password || url.search || url.hash) {
    throw Error('请输入 HTTPS Mario 网页地址；仅本机测试允许 HTTP。');
  }
  return [mode, site];
}
function run(command, args) {
  return new Promise((done, reject) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit', shell: false, windowsHide: true });
    child.once('error', reject);
    child.once('exit', code => code === 0 ? done() : reject(Error(`本机进程退出 (${code})`)));
  });
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = workerArguments(process.argv.slice(2));
    await run('cargo', ['build', '--manifest-path', 'server/Cargo.toml', '--bin', 'mario-server']);
    await run(resolve(root, `server/target/debug/mario-server${process.platform === 'win32' ? '.exe' : ''}`), args);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
