// Local only: Mario owns an official ChatGPT OAuth grant on this computer
// and never sends OpenAI credentials to the Mario site.
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { existsSync, realpathSync } from 'node:fs';
import { root, cargoTargetDir, serverBinary } from './server-artifacts.mjs';

export function workerArguments(input) {
  const args = [...input];
  const mode = args[0]?.startsWith('--') ? args.shift() : '--chatgpt-bridge';
  if (mode === '--codex-bridge') {
    throw Error('已移除旧版 Codex CLI 桥接，请改用 npm run chatgpt:bridge。');
  }
  if (mode === '--chatgpt-auth') {
    const [action, ...options] = args;
    const valid = (action === 'login' && (options.length === 0 || (options.length === 1 && options[0] === '--new') || (options.length === 2 && options[0] === '--account' && options[1]))) ||
      (action === 'status' && options.length === 0) ||
      (action === 'select' && options.length === 1 && options[0]) ||
      (action === 'logout' && options.length <= 1);
    if (!valid) throw Error('用法：npm run chatgpt:auth -- login [--new | --account <注册 ID>] | status | select <注册 ID> | logout [注册 ID]');
    return [mode, ...args];
  }
  if (mode !== '--chatgpt-bridge') throw Error('未知桥接启动模式。');
  const explicitApi = args[0] === '--api-url';
  const address = args[explicitApi ? 1 : 0];
  if (!address || args.length !== (explicitApi ? 2 : 1)) {
    throw Error('请复制 Mario 设置页的桥接启动命令，或运行 npm run chatgpt:bridge -- <生成配对码的 Mario 网页地址>。');
  }
  const url = new URL(address);
  if ((url.protocol !== 'https:' && !(url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname))) || url.username || url.password || url.search || url.hash) {
    throw Error('请输入不含凭据、查询或片段的 HTTPS Mario 地址；仅本机测试允许 HTTP。');
  }
  return [mode, ...args];
}
function run(command, args) {
  return new Promise((done, reject) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit', shell: false, windowsHide: true });
    child.once('error', reject);
    child.once('exit', code => code === 0 ? done() : reject(Error(`本机进程退出 (${code})`)));
  });
}
if (process.argv[1] && existsSync(process.argv[1]) && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  try {
    const args = workerArguments(process.argv.slice(2));
    const target = cargoTargetDir();
    await run('cargo', ['build', '--manifest-path', 'server/Cargo.toml', '--target-dir', target, '--bin', 'mario-server']);
    await run(serverBinary({ targetDir: target, binary: '' }), args);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
