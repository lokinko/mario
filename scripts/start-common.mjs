import { randomBytes } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync, renameSync, chmodSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export const secret = () => randomBytes(32).toString('hex');
export function defaults(domain = 'localhost') {
  return {
    MARIO_DOMAIN: domain,
    MARIO_SERVER_IP: '',
    MARIO_BASE_PATH: '/',
    POSTGRES_PASSWORD: secret(),
    MARIO_MASTER_KEY: randomBytes(32).toString('base64'),
    MARIO_REGISTRATION_KEY: secret(),
    MARIO_AGENT_TOKEN: secret(),
    MARIO_MODEL_HOSTS: 'api.openai.com,api.anthropic.com',
    MARIO_ALLOWED_ORIGINS: '',
    DATABASE_URL: '',
    PG_BIN: '',
    MARIO_PORT: '4217',
    MARIO_AGENT_PORT: '4218',
    MARIO_PG_PORT: '55432',
    MARIO_BACKUP_MODE: 'native',
    MARIO_BACKUP_DIR: 'backups',
    MARIO_BACKUP_SSH_HOST: '',
    MARIO_BACKUP_REMOTE_DIR: '',
    MARIO_BACKUP_SSH_PORT: '22',
    MARIO_BACKUP_SSH_IDENTITY: '',
  };
}

// Merge only requested non-secret settings; retain all existing keys and passwords.
export function updateConfig(path, values) {
  const remaining = new Map(Object.entries(values));
  for (const [key, value] of remaining) {
    if (!/^[A-Z_][A-Z0-9_]*$/.test(key) || /[\r\n\0]/.test(value)) throw Error('配置名称或值无效');
  }
  const content = readFileSync(path, 'utf8').replace(/\r\n/g, '\n').replace(/^([A-Z_][A-Z0-9_]*)=.*$/gm, (line, key) => {
    if (!remaining.has(key)) return line;
    const value = remaining.get(key); remaining.delete(key); return `${key}=${value}`;
  }).trimEnd() + '\n' + [...remaining].map(([key, value]) => `${key}=${value}\n`).join('');
  if (content !== readFileSync(path, 'utf8')) {
    const temporary = `${path}.${randomBytes(8).toString('hex')}.part`;
    writeFileSync(temporary, content, { flag: 'wx', mode: 0o600 });
    renameSync(temporary, path);
  }
  chmodSync(path, 0o600);
}

export function postgresBin(configured = '') {
  if (configured) return configured;
  if (existsSync('/usr/lib/postgresql')) {
    const dataVersion = resolve(root, '.runtime/native/postgres/PG_VERSION');
    const required = existsSync(dataVersion) ? readFileSync(dataVersion, 'utf8').trim() : undefined;
    const versions = readdirSync('/usr/lib/postgresql').filter(v => /^\d+$/.test(v)).sort((a, b) => Number(b) - Number(a));
    for (const version of versions) {
      if (required && version !== required) continue;
      const path = resolve('/usr/lib/postgresql', version, 'bin');
      if (existsSync(resolve(path, 'pg_ctl')) && existsSync(resolve(path, 'initdb'))) return path;
    }
    if (required) throw Error(`找不到现有数据对应的 PostgreSQL ${required} 工具；请安装该版本并配置 PG_BIN，不能直接用新主版本打开旧数据目录`);
  }
  return '';
}

// Configuration is data, never shell code. Accept plain or quoted dotenv values.
export function readConfig(path) {
  const result = {};
  for (const [index, line] of readFileSync(path, 'utf8').split(/\r?\n/).entries()) {
    if (!line.trim() || line.trimStart().startsWith('#')) continue;
    const match = line.match(/^\s*([A-Z_][A-Z0-9_]*)\s*=\s*(.*?)\s*$/);
    if (!match) throw Error(`${path}:${index + 1}: 无效配置行`);
    let value = match[2];
    if (value.startsWith('"') || value.startsWith("'")) {
      if (!value.endsWith(value[0]) || value.length < 2) throw Error(`${path}:${index + 1}: 引号不匹配`);
      value = value.slice(1, -1);
    }
    result[match[1]] = value;
  }
  return result;
}

export function createConfig(path, values) {
  if (existsSync(path)) return false;
  for (const value of Object.values(values)) {
    if (/[\r\n]/.test(value)) throw Error('配置值不能包含换行');
  }
  const remaining = new Map(Object.entries(values));
  const content = readFileSync(resolve(root, '.env.example'), 'utf8').replace(/\r\n/g, '\n').replace(/^([A-Z_][A-Z0-9_]*)=.*$/gm, (line, key) => {
    if (!remaining.has(key)) return line;
    const value = remaining.get(key);
    remaining.delete(key);
    return `${key}=${value}`;
  });
  writeFileSync(path, content.trimEnd() + '\n' + [...remaining].map(([key, value]) => `${key}=${value}\n`).join(''), { flag: 'wx', mode: 0o600 });
  console.log(`已创建 ${path}；请备份并妥善保管，后续启动不会覆盖。`);
  return true;
}

export function migrateNativeConfig() {
  const legacy = resolve(root, '.env.native');
  const target = resolve(root, '.env');
  const backup = resolve(root, '.env.native.backup');
  if (!existsSync(legacy)) return;
  if (existsSync(target)) throw Error('同时存在 .env 与旧 .env.native；请核对并将旧配置合并到 .env，再将 .env.native 移至备份位置，避免使用错误密钥');
  if (existsSync(backup)) throw Error('已有 .env.native.backup，请先整理旧配置备份');
  const previous = readConfig(legacy);
  validateSecrets(previous);
  if (!previous.POSTGRES_PASSWORD) throw Error('旧配置缺少 POSTGRES_PASSWORD，请先补齐');
  createConfig(target, { ...defaults(), ...previous });
  renameSync(legacy, backup);
  console.log('旧原生配置已合并到 .env，原文件保留为 .env.native.backup；密钥与数据未更换。');
}

export function createDeploymentConfig(values) {
  migrateNativeConfig();
  const path = resolve(root, '.env');
  if (!existsSync(path) && existsSync(resolve(root, '.runtime/native/postgres/PG_VERSION'))) throw Error('已有本地数据库但缺少 .env；请恢复原配置，不能重新生成加密密钥');
  return createConfig(path, values);
}

export function assertNoRestore() {
  if (existsSync(resolve(root, '.runtime/restore.lock'))) throw Error('数据恢复正在进行或存在遗留 restore.lock；请确认恢复完成后再启动服务');
}

export function args(allowed) {
  const result = {};
  const input = process.argv.slice(2);
  for (let i = 0; i < input.length; i++) {
    const name = input[i];
    if (!(name in allowed)) throw Error(`未知参数：${name}；使用 --help 查看帮助`);
    result[name] = allowed[name] ? input[++i] : true;
    if (allowed[name] && (!result[name] || result[name].startsWith('--'))) throw Error(`${name} 缺少参数`);
  }
  return result;
}

export function hostname(value) {
  if (!/^(localhost|(?=.{1,253}$)[a-zA-Z0-9](?:[a-zA-Z0-9.-]*[a-zA-Z0-9])?)$/.test(value)) throw Error('域名仅填写主机名，不包含 https://、端口或路径');
  return value;
}

export function basePath(value = '/') {
  if (!/^\/(?:[A-Za-z0-9_-]+\/)*$/.test(value)) throw Error('MARIO_BASE_PATH 必须为 / 或 /mario/ 这样的路径，且以 / 结尾');
  return value;
}

export function port(value, name) {
  if (!/^\d+$/.test(String(value)) || Number(value) < 1 || Number(value) > 65535) throw Error(`${name} 必须在 1–65535 之间`);
  return Number(value);
}

export function run(command, commandArgs, options = {}) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, commandArgs, { cwd: root, stdio: 'inherit', ...options });
    child.on('error', error => reject(Error(`无法执行 ${command}: ${error.message}`)));
    child.on('exit', (code, signal) => code === 0 ? resolveRun() : reject(Error(`${command} 失败 (${signal ?? code})`)));
  });
}

export function validateSecrets(config) {
  if (Buffer.from(config.MARIO_MASTER_KEY ?? '', 'base64').length !== 32) throw Error('MARIO_MASTER_KEY 必须是 Base64 编码的 32 字节密钥');
  for (const key of ['MARIO_REGISTRATION_KEY', 'MARIO_AGENT_TOKEN']) {
    if ((config[key] ?? '').length < 32) throw Error(`${key} 至少需要 32 字符`);
  }
}
