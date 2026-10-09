import { spawn } from 'node:child_process';
import { randomBytes, scrypt as scryptCallback, createCipheriv, createDecipheriv, createHash } from 'node:crypto';
import { promisify } from 'node:util';
import { pipeline } from 'node:stream/promises';
import { Readable } from 'node:stream';
import { createReadStream, createWriteStream, existsSync, readFileSync, mkdirSync } from 'node:fs';
import { open, stat, unlink, link, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join, resolve, dirname, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { readConfig, root, port, postgresBin } from './start-common.mjs';

const magic = Buffer.from('MARIOBK1');
const scrypt = promisify(scryptCallback);
export const timestamp = () => new Date().toISOString().replace(/[:.]/g, '-');
export const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
export const schemasSql = "SELECT nspname FROM pg_namespace WHERE nspname='mario_auth' OR nspname ~ '^user_[0-9a-f]{32}$' ORDER BY nspname";
export const schemaName = name => name === 'mario_auth' || /^user_[0-9a-f]{32}$/.test(name);
export const masterKey = value => typeof value === 'string' && Buffer.from(value, 'base64').length === 32;

export async function workspace() {
  const path = await mkdtemp(join(tmpdir(), 'mario-backup-'));
  return { path, dispose: () => {
    if (!resolve(path).startsWith(resolve(tmpdir()) + sep + 'mario-backup-')) throw Error('拒绝清理非本次创建的临时目录');
    return rm(path, { recursive: true, force: true });
  } };
}

// No shell, no passwords in command arguments, no unrestricted command output.
export async function command(program, argv, { env = process.env, input, output, capture = false, accepted = [0] } = {}) {
  const child = spawn(program, argv, { cwd: root, env, stdio: ['pipe', 'pipe', 'pipe'] });
  let stdout = '', stderr = '';
  child.stderr.on('data', chunk => { stderr = (stderr + chunk.toString()).slice(-4000); });
  if (!output) child.stdout.on('data', chunk => { if (capture) stdout += chunk; });
  const done = new Promise((resolveDone, reject) => {
    child.once('error', () => reject(Error(`无法执行 ${program}；请检查工具安装与路径`)));
    child.once('exit', code => accepted.includes(code) ? resolveDone(code) : reject(Error(`${program} 执行失败（${code}）。请检查数据库连接、工具版本和权限。${stderr.includes('password authentication failed') ? ' 数据库密码不匹配。' : ''}`)));
  });
  const tasks = [done];
  if (output) tasks.push(pipeline(child.stdout, createWriteStream(output, { flags: 'wx', mode: 0o600 })));
  if (input) tasks.push(pipeline(createReadStream(input), child.stdin));
  else child.stdin.end();
  try { const results = await Promise.all(tasks); return capture ? stdout.trim() : results[0]; }
  catch (error) { if (child.exitCode === null) child.kill(); await Promise.allSettled(tasks); throw error; }
}

export async function password(options, confirm = false) {
  let value;
  if (options['--password-file']) value = readFileSync(resolve(options['--password-file']), 'utf8').replace(/\r?\n$/, '');
  else if (options['--stdin-password']) {
    let input = '';
    for await (const chunk of process.stdin) {
      input += chunk;
      if (input.length > 4096) throw Error('密码输入过长');
    }
    value = input.replace(/\r?\n$/, '');
  } else if (process.env.MARIO_BACKUP_PASSWORD) value = process.env.MARIO_BACKUP_PASSWORD;
  else {
    value = await hiddenInput('备份加密密码（至少 12 字符）：');
    if (confirm && value !== await hiddenInput('再次输入密码：')) throw Error('两次密码不一致');
  }
  if (value.length < 12 || value.length > 1024 || /[\r\n\0]/.test(value)) throw Error('备份密码需为 12–1024 个字符，不能包含换行');
  return value;
}

function hiddenInput(label) {
  if (!process.stdin.isTTY || !process.stdin.setRawMode) throw Error('非交互运行请设置 MARIO_BACKUP_PASSWORD，或传入 --password-file');
  process.stderr.write(label);
  return new Promise((resolveInput, reject) => {
    let value = '';
    const previousRaw = process.stdin.isRaw;
    process.stdin.setRawMode(true); process.stdin.resume();
    function finish(error) {
      process.stdin.off('data', read);
      process.stdin.setRawMode(previousRaw); process.stdin.pause();
      process.stderr.write('\n');
      error ? reject(error) : resolveInput(value);
    }
    function read(chunk) {
      for (const char of chunk.toString('utf8')) {
        if (char === '\u0003') return finish(Error('已取消'));
        if (char === '\r' || char === '\n') return finish();
        if (char === '\u007f' || char === '\b') value = value.slice(0, -1);
        else if (char >= ' ') value += char;
        if (value.length > 1024) return finish(Error('密码过长'));
      }
    }
    process.stdin.on('data', read);
  });
}

async function derive(passphrase, salt) {
  return scrypt(passphrase, salt, 32, { N: 131072, r: 8, p: 1, maxmem: 256 * 1024 * 1024 });
}
async function hashFile(path) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

export async function seal(dump, metadata, passphrase, destination) {
  const path = resolve(destination);
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  if (existsSync(path)) throw Error(`输出文件已存在，不会覆盖：${path}`);
  const temporary = `${path}.${randomBytes(8).toString('hex')}.part`;
  const salt = randomBytes(16), nonce = randomBytes(12);
  const header = Buffer.concat([magic, salt, nonce]);
  const cipher = createCipheriv('aes-256-gcm', await derive(passphrase, salt), nonce);
  cipher.setAAD(header);
  const manifest = Buffer.from(JSON.stringify({ ...metadata, format: 1, sha256: await hashFile(dump), dumpBytes: (await stat(dump)).size }), 'utf8');
  const length = Buffer.alloc(4); length.writeUInt32BE(manifest.length);
  async function* payload() { yield length; yield manifest; yield* createReadStream(dump); }
  try {
    const file = await open(temporary, 'wx', 0o600);
    await file.write(header); await file.close();
    await pipeline(Readable.from(payload()), cipher, createWriteStream(temporary, { flags: 'a', mode: 0o600 }));
    const tail = await open(temporary, 'a'); await tail.write(cipher.getAuthTag()); await tail.close();
    // Link publishes a complete file atomically without overwriting a concurrent export.
    await link(temporary, path);
    return path;
  } finally { await unlink(temporary).catch(() => {}); }
}

export async function unseal(archive, passphrase, directory) {
  const size = (await stat(archive)).size;
  if (size < 56) throw Error('迁移包不完整');
  const file = await open(archive, 'r');
  const header = Buffer.alloc(36), tag = Buffer.alloc(16);
  try { await file.read(header, 0, 36, 0); await file.read(tag, 0, 16, size - 16); }
  finally { await file.close(); }
  if (!header.subarray(0, 8).equals(magic)) throw Error('不是 Mario 加密迁移包，或格式版本不支持');
  const decipher = createDecipheriv('aes-256-gcm', await derive(passphrase, header.subarray(8, 24)), header.subarray(24));
  decipher.setAAD(header); decipher.setAuthTag(tag);
  const plain = join(directory, 'payload'), dump = join(directory, 'database.dump');
  try { await pipeline(createReadStream(archive, { start: 36, end: size - 17 }), decipher, createWriteStream(plain, { flags: 'wx', mode: 0o600 })); }
  catch { await unlink(plain).catch(() => {}); throw Error('密码错误或迁移包损坏；没有修改目标数据'); }
  const input = await open(plain, 'r');
  let metadata, length;
  try {
    const prefix = Buffer.alloc(4); await input.read(prefix, 0, 4, 0); length = prefix.readUInt32BE();
    if (length < 1 || length > 65536) throw Error('迁移包元数据无效');
    const json = Buffer.alloc(length); const result = await input.read(json, 0, length, 4);
    if (result.bytesRead !== length) throw Error('迁移包元数据不完整');
    metadata = JSON.parse(json.toString('utf8'));
  } finally { await input.close(); }
  if (metadata.app !== 'mario' || metadata.format !== 1 || typeof metadata.version !== 'string' || !Number.isInteger(metadata.serverMajor) || metadata.serverMajor < 12 || !Number.isSafeInteger(metadata.dumpBytes) || metadata.dumpBytes < 1 || !/^[0-9a-f]{64}$/.test(metadata.sha256) || !masterKey(metadata.masterKey) || !Array.isArray(metadata.schemas) || !metadata.schemas.every(schemaName) || !metadata.schemas.includes('mario_auth')) throw Error('迁移包内容不符合 Mario 数据契约');
  await pipeline(createReadStream(plain, { start: length + 4 }), createWriteStream(dump, { flags: 'wx', mode: 0o600 }));
  await unlink(plain);
  if ((await stat(dump)).size !== metadata.dumpBytes || await hashFile(dump) !== metadata.sha256) throw Error('迁移包数据校验失败');
  return { dump, metadata };
}

export async function database(modeOption, { temporaryStart = true, initialize = false } = {}) {
  const path = join(root, '.env');
  if (!existsSync(path)) throw Error('缺少根目录 .env，请先配置目标服务器');
  const config = readConfig(path);
  if (!masterKey(config.MARIO_MASTER_KEY)) throw Error('.env 中 MARIO_MASTER_KEY 无效');
  let mode = modeOption ?? config.MARIO_BACKUP_MODE ?? 'auto';
  const data = join(root, '.runtime/native/postgres');
  const uri = process.env.DATABASE_URL || config.DATABASE_URL;
  if (mode === 'auto') mode = 'native';
  if (mode !== 'native') throw Error('当前版本只支持宿主机 native 备份；旧 Docker 实例请用旧版本导出加密包，再在本版本恢复');
  let ownsPostgres = false;
  const baseEnv = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^(PG|DATABASE_URL$|MARIO_|POSTGRES_)/i.test(key)));
  const pgBin = postgresBin(process.env.PG_BIN || config.PG_BIN);
  const pg = name => pgBin ? join(pgBin, name + (process.platform === 'win32' ? '.exe' : '')) : name;
  let connection, pgEnv = baseEnv;
  if (mode === 'native') {
    if (uri) {
      let url; try { url = new URL(uri); } catch { throw Error('DATABASE_URL 必须为 PostgreSQL URL'); }
      if (!['postgresql:', 'postgres:'].includes(url.protocol)) throw Error('DATABASE_URL 必须为 PostgreSQL URL');
      pgEnv = { ...baseEnv, PGPASSWORD: decodeURIComponent(url.password || url.searchParams.get('password') || '') };
      url.password = ''; url.searchParams.delete('password'); connection = url.toString();
    } else {
      if (!existsSync(join(data, 'PG_VERSION'))) {
        if (!initialize) throw Error('本地数据库尚未初始化；请先运行原生启动脚本，或设置 DATABASE_URL');
        if (process.getuid?.() === 0) throw Error('本地 PostgreSQL 初始化不能使用 root，请用普通用户运行');
        if (!config.POSTGRES_PASSWORD) throw Error('.env 缺少 POSTGRES_PASSWORD');
        mkdirSync(join(root, '.runtime/native'), { recursive: true, mode: 0o700 });
        const passwordFile = join(root, '.runtime/native', `restore-password-${randomBytes(8).toString('hex')}`);
        await writeFile(passwordFile, config.POSTGRES_PASSWORD, { flag: 'wx', mode: 0o600 });
        try { await command(pg('initdb'), ['-D', data, '-U', 'mario', '--auth-host=scram-sha-256', '--auth-local=scram-sha-256', '--encoding=UTF8', '--no-locale', `--pwfile=${passwordFile}`], { env: baseEnv }); }
        finally { await unlink(passwordFile); }
      }
      pgEnv = { ...baseEnv, PGPASSWORD: config.POSTGRES_PASSWORD };
      connection = `postgresql://mario@127.0.0.1:${port(config.MARIO_PG_PORT ?? '55432', 'MARIO_PG_PORT')}/postgres`;
      if (temporaryStart) {
        const status = await command(pg('pg_ctl'), ['-D', data, 'status'], { env: baseEnv, accepted: [0, 3] });
        if (status === 3) {
          await command(pg('pg_ctl'), ['-D', data, '-l', join(root, '.runtime/native/postgres.log'), '-w', '-t', '30', '-o', `-h 127.0.0.1 -p ${port(config.MARIO_PG_PORT ?? '55432', 'MARIO_PG_PORT')}`, 'start'], { env: baseEnv });
          ownsPostgres = true;
        }
      }
    }
  }
  const db = {
    mode, config,
    async tool(name, argv, options = {}) {
      return command(pg(name), ['--dbname', connection, ...argv], { ...options, env: pgEnv });
    },
    async schemas() { return (await this.tool('psql', ['-X', '-tA', '-v', 'ON_ERROR_STOP=1', '-c', schemasSql], { capture: true })).split(/\r?\n/).filter(Boolean); },
    async serverMajor() { return Math.floor(Number(await this.tool('psql', ['-X', '-tA', '-c', 'SHOW server_version_num'], { capture: true })) / 10000); },
    async dump(destination) {
      const schemas = await this.schemas();
      if (!schemas.includes('mario_auth')) throw Error('目标缺少 Mario 账号 schema，无法创建完整备份');
      const foreign = await this.tool('psql', ['-X', '-tA', '-c', "SELECT nspname FROM pg_namespace WHERE left(nspname,5)='user_' AND nspname !~ '^user_[0-9a-f]{32}$'"], { capture: true });
      if (foreign) throw Error('数据库中有其他 user_ 前缀 schema，请将 Mario 使用的数据库与其他应用隔离后备份');
      // Let pg_dump enumerate all tenants inside its own consistent transaction;
      // a list captured before that snapshot could miss a newly registered tenant.
      await this.tool('pg_dump', ['--format=custom', '--no-owner', '--no-privileges', '--schema=mario_auth', '--schema=user_*'], { output: destination });
    },
    async restoreSql(dump, destination) {
      const argv = ['--no-owner', '--no-privileges', '--file=-'];
      return command(pg('pg_restore'), argv, { env: pgEnv, input: dump, output: destination });
    },
    async close() { if (ownsPostgres) await command(pg('pg_ctl'), ['-D', data, '-w', '-m', 'fast', 'stop'], { env: baseEnv }); },
  };
  return db;
}

export async function makeBackup(db, passphrase, destination, directory) {
  const schemas = await db.schemas();
  if (!schemas.includes('mario_auth')) throw Error('数据库中没有 Mario 账号表，无法打包');
  const dump = join(directory, `dump-${randomBytes(6).toString('hex')}`);
  try {
    await db.dump(dump);
    if (readConfig(join(root, '.env')).MARIO_MASTER_KEY !== db.config.MARIO_MASTER_KEY) throw Error('备份期间主密钥发生变化，请停止修改配置后重试');
    return await seal(dump, { app: 'mario', version, createdAt: new Date().toISOString(), serverMajor: await db.serverMajor(), sourceMode: db.mode, schemas, masterKey: db.config.MARIO_MASTER_KEY }, passphrase, destination);
  } finally { await unlink(dump).catch(() => {}); }
}
