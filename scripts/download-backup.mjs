import { spawn } from 'node:child_process';
import { pipeline } from 'node:stream/promises';
import { createWriteStream, mkdirSync, existsSync } from 'node:fs';
import { link, unlink } from 'node:fs/promises';
import { dirname, resolve, join } from 'node:path';
import { randomBytes } from 'node:crypto';
import { args, root, port, readConfig } from './start-common.mjs';
import { password, timestamp, unseal, workspace } from './backup-common.mjs';

const quote = value => `'${value.replace(/'/g, "'\\''")}'`;
let temporary, work;
try {
  const options = args({ '--help': false, '--host': true, '--remote-dir': true, '--mode': true, '--output': true, '--port': true, '--identity': true, '--password-file': true });
  if (options['--help']) {
    console.log(`用法：node scripts/download-backup.mjs --host user@server --remote-dir /srv/mario [--mode docker|native] [--output 本地文件.mario-backup]
在本机运行，通过 SSH 在 Linux/macOS 源服务器打包并直接下载，不在远端保留迁移包。
需要本机 ssh、远端 Node.js 20+ 与本项目。支持 --port 和 --identity；遵循 SSH 主机密钥验证。
备份加密密码单独输入，通过 SSH stdin 传递，不进入命令参数或远端配置。`);
  } else {
    const config = existsSync(join(root, '.env')) ? readConfig(join(root, '.env')) : {};
    const host = options['--host'] || config.MARIO_BACKUP_SSH_HOST;
    const directory = options['--remote-dir'] || config.MARIO_BACKUP_REMOTE_DIR;
    const mode = options['--mode'] || config.MARIO_BACKUP_MODE || 'auto';
    if (!host || !/^[A-Za-z0-9_.@:[\]-]+$/.test(host) || host.startsWith('-')) throw Error('--host 请填写 user@server 或 SSH 配置别名');
    if (!directory || /[\r\n\0]/.test(directory)) throw Error('--remote-dir 请填写远端项目路径');
    if (!['auto', 'native', 'docker'].includes(mode)) throw Error('--mode 应为 auto、native 或 docker');
    const passphrase = await password(options, true);
    const destination = resolve(options['--output'] ?? join(root, config.MARIO_BACKUP_DIR || 'backups', `download-${timestamp()}.mario-backup`));
    if (existsSync(destination)) throw Error(`输出文件已存在，不会覆盖：${destination}`);
    mkdirSync(dirname(destination), { recursive: true, mode: 0o700 });
    temporary = `${destination}.${randomBytes(8).toString('hex')}.part`;
    const argv = ['-T'];
    argv.push('-p', String(port(options['--port'] || config.MARIO_BACKUP_SSH_PORT || '22', 'SSH port')));
    const identity = options['--identity'] || config.MARIO_BACKUP_SSH_IDENTITY;
    if (identity) argv.push('-i', resolve(identity));
    argv.push(host, `cd ${quote(directory)} && node scripts/backup.mjs --stdout --stdin-password${mode !== 'auto' ? ` --mode ${quote(mode)}` : ''}`);
    const child = spawn('ssh', argv, { stdio: ['pipe', 'pipe', 'inherit'] });
    const done = new Promise((resolveDone, reject) => {
      child.once('error', () => reject(Error('无法启动 ssh，请检查 OpenSSH 客户端')));
      child.once('exit', code => code === 0 ? resolveDone() : reject(Error(`SSH 打包/下载失败 (${code})`)));
    });
    child.stdin.on('error', () => {});
    child.stdin.end(passphrase + '\n');
    const transfer = pipeline(child.stdout, createWriteStream(temporary, { flags: 'wx', mode: 0o600 }));
    try { await Promise.all([done, transfer]); }
    catch (error) { if (child.exitCode === null) child.kill(); await Promise.allSettled([done, transfer]); throw error; }
    work = await workspace();
    await unseal(temporary, passphrase, work.path);
    await link(temporary, destination);
    console.log(`下载完成：${destination}\n这是加密迁移包，目标服务器可用 restore-backup.mjs 恢复。`);
  }
} catch (error) { console.error(`下载失败：${error.message}`); process.exitCode = 1; }
finally { if (temporary) await unlink(temporary).catch(() => {}); await work?.dispose(); }
