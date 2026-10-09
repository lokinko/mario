import { existsSync, mkdirSync, readFileSync, writeFileSync, unlinkSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { setTimeout as sleep } from 'node:timers/promises';
import { isIP } from 'node:net';
import { args, assertNoRestore, basePath, createDeploymentConfig, defaults, hostname, migrateNativeConfig, port, postgresBin, readConfig, root, run, updateConfig, validateSecrets } from './start-common.mjs';

const runtime = resolve(root, '.runtime/native');
const configPath = resolve(root, '.env');
const lockPath = join(runtime, 'launcher.lock');
const children = [];
let ownsLock = false, managedPostgres = false, stopping = false, pg, pgData, baseEnv;
let notifyStop;
const stopRequested = new Promise(resolveStop => { notifyStop = resolveStop; });
function requestStop() { stopping = true; notifyStop(); }
function ensureRunning() { if (stopping) throw Error('启动已取消'); }
function cleanEnvironment() {
  return Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^(MARIO_|DATABASE_URL$|POSTGRES_|PGPASSWORD$|PGSERVICE|PGOPTIONS$|PGPASSFILE$|VITE_)/i.test(key)));
}
function launch(name, command, commandArgs, env) {
  ensureRunning();
  const child = spawn(command, commandArgs, { cwd: root, env, stdio: 'inherit' });
  child.once('spawn', () => console.log(`${name} PID ${child.pid}`));
  const entry = { child, name, done: null, exited: false };
  entry.done = new Promise(resolveExit => {
    child.on('error', error => { console.error(`${name} 无法启动：${error.message}`); process.exitCode = 1; requestStop(); resolveExit(); });
    child.on('exit', (code, signal) => {
      entry.exited = true;
      if (!stopping) { console.error(`${name} 意外退出 (${signal ?? code})`); process.exitCode = 1; requestStop(); }
      resolveExit();
    });
  });
  children.push(entry);
}
async function freePort(number) {
  await new Promise((resolvePort, reject) => {
    const server = createServer();
    server.once('error', () => reject(Error(`127.0.0.1:${number} 已被占用；停止旧进程或在 .env 中修改端口`)));
    server.listen(number, '127.0.0.1', () => server.close(resolvePort));
  });
}
async function ready(url, expected, headers = {}) {
  for (let attempt = 0; attempt < 120; attempt++) {
    ensureRunning();
    try {
      const response = await fetch(url, { headers, signal: AbortSignal.timeout(2000) });
      if (response.ok && (!expected || expected(await response.json()))) return;
    } catch { /* Process may still be starting. */ }
    await sleep(500);
  }
  throw Error(`服务未就绪：${url}`);
}
function acquireLock() {
  mkdirSync(runtime, { recursive: true, mode: 0o700 });
  if (existsSync(lockPath)) {
    const pid = Number(readFileSync(lockPath, 'utf8').trim());
    if (!Number.isSafeInteger(pid) || pid < 1) throw Error(`无效锁文件，请核对后手动删除 ${lockPath}`);
    try { process.kill(pid, 0); throw Error(`已有启动进程 PID ${pid}；请先停止它`); }
    catch (error) { if (error.code !== 'ESRCH') throw error; }
    unlinkSync(lockPath);
  }
  writeFileSync(lockPath, String(process.pid), { flag: 'wx', mode: 0o600 });
  ownsLock = true;
}

try {
  const options = args({ '--help': false, '--prepare': false, '--build': false, '--skip-build': false });
  if (options['--help']) {
    console.log('内部运行器；服务器部署请使用项目根目录 bash deploy.sh --help。');
  } else if (options['--prepare']) {
    assertNoRestore();
    migrateNativeConfig();
    const original = existsSync(configPath) ? readConfig(configPath) : {};
    const setup = process.env;
    const domain = hostname(setup.MARIO_SETUP_DOMAIN || original.MARIO_DOMAIN || 'localhost');
    const ip = setup.MARIO_SETUP_IP || original.MARIO_SERVER_IP || '';
    if (ip && !isIP(ip)) throw Error('公网 IP 格式无效');
    const prefix = basePath(setup.MARIO_SETUP_BASE_PATH || original.MARIO_BASE_PATH);
    const values = {
      MARIO_DOMAIN: domain, MARIO_SERVER_IP: ip, MARIO_BASE_PATH: prefix,
      MARIO_PORT: String(port(setup.MARIO_SETUP_API_PORT || original.MARIO_PORT || '4217', 'MARIO_PORT')),
      MARIO_AGENT_PORT: String(port(setup.MARIO_SETUP_AGENT_PORT || original.MARIO_AGENT_PORT || '4218', 'MARIO_AGENT_PORT')),
      MARIO_PG_PORT: String(port(setup.MARIO_SETUP_PG_PORT || original.MARIO_PG_PORT || '55432', 'MARIO_PG_PORT')),
      PG_BIN: postgresBin(setup.MARIO_SETUP_PG_BIN || original.PG_BIN),
      MARIO_BACKUP_MODE: 'native',
    };
    if (setup.MARIO_SETUP_DATABASE_URL) values.DATABASE_URL = setup.MARIO_SETUP_DATABASE_URL;
    if (setup.MARIO_SETUP_MODEL_HOSTS) values.MARIO_MODEL_HOSTS = setup.MARIO_SETUP_MODEL_HOSTS;
    if (setup.MARIO_SETUP_ORIGINS) values.MARIO_ALLOWED_ORIGINS = setup.MARIO_SETUP_ORIGINS;
    if (new Set([values.MARIO_PORT, values.MARIO_AGENT_PORT, values.MARIO_PG_PORT]).size !== 3) throw Error('API、Agent、PostgreSQL 必须使用不同端口');
    createDeploymentConfig({ ...defaults(domain), ...values });
    validateSecrets(readConfig(configPath));
    updateConfig(configPath, values);
    mkdirSync(runtime, { recursive: true, mode: 0o700 });
    const mount = prefix === '/' ? '' : prefix.slice(0, -1);
    const redirect = mount ? `location = ${mount} { return 308 ${prefix}$is_args$args; }\n` : '';
    const nginx = `${redirect}location ^~ ${prefix} {\n    proxy_pass http://127.0.0.1:${values.MARIO_PORT}/;\n    proxy_http_version 1.1;\n    proxy_set_header Host $host;\n    proxy_set_header X-Real-IP $remote_addr;\n    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n    proxy_set_header X-Forwarded-Proto $scheme;\n    proxy_read_timeout 300s;\n    client_max_body_size 8m;\n    add_header X-Content-Type-Options nosniff always;\n    add_header X-Frame-Options DENY always;\n    add_header Referrer-Policy same-origin always;\n    add_header Content-Security-Policy "default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self'; frame-ancestors 'none'" always;\n}\n`;
    writeFileSync(join(runtime, 'nginx.conf'), nginx);
    console.log(`配置已准备：域名 ${domain}，网页路径 ${prefix}${ip ? `，DNS 目标 ${ip}` : ''}\nNginx 配置：${join(runtime, 'nginx.conf')}（加入现有 HTTPS server）`);
  } else if (options['--build']) {
    assertNoRestore();
    const config = readConfig(configPath);
    validateSecrets(config);
    const env = { ...cleanEnvironment(), VITE_BASE_PATH: basePath(config.MARIO_BASE_PATH) };
    await run(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['ci', '--prefix', 'client'], { env, shell: process.platform === 'win32' });
    await run(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['run', 'build', '--prefix', 'client'], { env, shell: process.platform === 'win32' });
    await run('cargo', ['build', '--release', '--locked', '--manifest-path', 'server/Cargo.toml'], { env });
  } else {
    acquireLock();
    assertNoRestore();
    process.on('SIGINT', requestStop);
    process.on('SIGTERM', requestStop);
    baseEnv = cleanEnvironment();
    migrateNativeConfig();
    if (!existsSync(configPath) && existsSync(join(runtime, 'postgres/PG_VERSION'))) throw Error('已有本地数据库但缺少 .env；请恢复原配置，不能重新生成加密密钥');
    const original = existsSync(configPath) ? readConfig(configPath) : {};
    const externalDatabase = process.env.DATABASE_URL || original.DATABASE_URL;
    const pgBin = postgresBin(process.env.PG_BIN || original.PG_BIN);
    pg = name => pgBin ? join(pgBin, name + (process.platform === 'win32' ? '.exe' : '')) : name;
    if (!externalDatabase) {
      if (process.getuid?.() === 0) throw Error('PostgreSQL 不允许 root 运行。请使用普通用户启动，或通过 DATABASE_URL 连接已部署的 PostgreSQL');
      await run(pg('initdb'), ['--version'], { env: baseEnv });
      await run(pg('pg_ctl'), ['--version'], { env: baseEnv });
    }
    if (!options['--skip-build']) {
      await run('cargo', ['--version'], { env: baseEnv });
      await run(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['--version'], { env: baseEnv, shell: process.platform === 'win32' });
    }
    ensureRunning();
    createDeploymentConfig({ ...defaults(), DATABASE_URL: externalDatabase ?? '', PG_BIN: pgBin ?? '' });
    const config = readConfig(configPath);
    validateSecrets(config);
    const prefix = basePath(config.MARIO_BASE_PATH);
    const apiPort = port(config.MARIO_PORT ?? '4217', 'MARIO_PORT');
    const agentPort = port(config.MARIO_AGENT_PORT ?? '4218', 'MARIO_AGENT_PORT');
    const pgPort = port(config.MARIO_PG_PORT ?? '55432', 'MARIO_PG_PORT');
    if (apiPort === agentPort || (!externalDatabase && [apiPort, agentPort].includes(pgPort))) throw Error('API、Agent、PostgreSQL 必须使用不同端口');
    const binary = resolve(root, 'server/target/release/mario-server' + (process.platform === 'win32' ? '.exe' : ''));
    if (!options['--skip-build']) {
      await run(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['ci', '--prefix', 'client'], { env: baseEnv, shell: process.platform === 'win32' });
      ensureRunning();
      await run(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['run', 'build', '--prefix', 'client'], { env: { ...baseEnv, VITE_BASE_PATH: prefix }, shell: process.platform === 'win32' });
      ensureRunning();
      await run('cargo', ['build', '--release', '--locked', '--manifest-path', 'server/Cargo.toml'], { env: baseEnv });
    }
    ensureRunning();
    if (!existsSync(binary) || !existsSync(resolve(root, 'client/dist/index.html'))) throw Error('缺少构建产物；去掉 --skip-build 重新运行');
    await freePort(apiPort);
    await freePort(agentPort);
    let databaseUrl = externalDatabase;
    if (!databaseUrl) {
      if (!config.POSTGRES_PASSWORD) throw Error('.env 缺少 POSTGRES_PASSWORD');
      await freePort(pgPort);
      pgData = join(runtime, 'postgres');
      if (!existsSync(join(pgData, 'PG_VERSION'))) {
        const passwordFile = join(runtime, 'init-password');
        writeFileSync(passwordFile, config.POSTGRES_PASSWORD, { flag: 'wx', mode: 0o600 });
        try {
          await run(pg('initdb'), ['-D', pgData, '-U', 'mario', '--auth-host=scram-sha-256', '--auth-local=scram-sha-256', '--encoding=UTF8', '--no-locale', `--pwfile=${passwordFile}`], { env: baseEnv });
        } finally { unlinkSync(passwordFile); }
      }
      ensureRunning();
      await run(pg('pg_ctl'), ['-D', pgData, '-l', join(runtime, 'postgres.log'), '-w', '-t', '30', '-o', `-h 127.0.0.1 -p ${pgPort}`, 'start'], { env: baseEnv });
      managedPostgres = true;
      // Use the initial postgres database; the application creates its own schemas.
      databaseUrl = `postgresql://mario:${encodeURIComponent(config.POSTGRES_PASSWORD)}@127.0.0.1:${pgPort}/postgres`;
    }
    const shared = { MARIO_AGENT_TOKEN: config.MARIO_AGENT_TOKEN, MARIO_MODEL_HOSTS: config.MARIO_MODEL_HOSTS ?? 'api.openai.com,api.anthropic.com' };
    launch('Agent', binary, [], { ...baseEnv, ...shared, MARIO_ROLE: 'agent', MARIO_AGENT_BIND: `127.0.0.1:${agentPort}` });
    await ready(`http://127.0.0.1:${agentPort}/health`, body => body.status === 'ok', { Authorization: `Bearer ${config.MARIO_AGENT_TOKEN}` });
    launch('API', binary, ['--port', String(apiPort)], {
      ...baseEnv, ...shared, DATABASE_URL: databaseUrl, MARIO_MASTER_KEY: config.MARIO_MASTER_KEY,
      MARIO_REGISTRATION_KEY: config.MARIO_REGISTRATION_KEY, MARIO_ALLOWED_ORIGINS: config.MARIO_ALLOWED_ORIGINS ?? '',
      MARIO_HOST: '127.0.0.1', MARIO_AGENT_URL: `http://127.0.0.1:${agentPort}`, MARIO_WEB_DIR: resolve(root, 'client/dist'),
    });
    await ready(`http://127.0.0.1:${apiPort}/api/server`, body => body.mode === 'hosted');
    console.log(`\nAPI 与 Agent 已就绪：http://127.0.0.1:${apiPort}\n公网网页：https://${config.MARIO_DOMAIN}${prefix}\n邀请码位于 .env 的 MARIO_REGISTRATION_KEY。\n前台运行器由 systemd 托管；Ctrl+C/SIGTERM 停止并保留数据。`);
    if (prefix !== '/') console.log(`公网网页路径：${prefix}（外层代理需移除该前缀，本机上游仍使用根路径）`);
    await stopRequested;
  }
} catch (error) {
  console.error(`启动失败：${error.message}`);
  process.exitCode = 1;
} finally {
  stopping = true;
  for (const entry of children.reverse()) {
    if (!entry.exited) entry.child.kill('SIGTERM');
    let timer;
    await Promise.race([entry.done, new Promise(resolveTimeout => { timer = setTimeout(resolveTimeout, 10000); })]);
    clearTimeout(timer);
    if (!entry.exited && entry.child.pid) { entry.child.kill('SIGKILL'); await entry.done; }
  }
  if (managedPostgres) {
    try { await run(pg('pg_ctl'), ['-D', pgData, '-w', '-t', '30', '-m', 'fast', 'stop'], { env: baseEnv }); }
    catch (error) { console.error(`数据库未正常停止：${error.message}；请检查 ${pgData}`); process.exitCode = 1; }
  }
  if (ownsLock) unlinkSync(lockPath);
}
