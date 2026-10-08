// Real native-launcher smoke test, using an isolated project and PostgreSQL cluster.
// Requires a built debug backend, client/dist and MARIO_TEST_PG_BIN (or PostgreSQL on PATH).
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdtempSync, mkdirSync, copyFileSync, cpSync, readFileSync, writeFileSync, existsSync, rmSync, unlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join, sep } from 'node:path';
import { createServer } from 'node:net';
import { setTimeout as sleep } from 'node:timers/promises';
import { root, defaults, createConfig, readConfig } from './start-common.mjs';

const fixture = mkdtempSync(join(tmpdir(), 'mario-start-'));
const executable = 'mario-server' + (process.platform === 'win32' ? '.exe' : '');
const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^(MARIO_|DATABASE_URL$|PG_BIN$)/i.test(key)));
if (process.env.MARIO_TEST_PG_BIN) env.PG_BIN = process.env.MARIO_TEST_PG_BIN;
const actualPgCtl = env.PG_BIN ? join(env.PG_BIN, 'pg_ctl' + (process.platform === 'win32' ? '.exe' : '')) : 'pg_ctl';
let current;
async function freePort() {
  const server = createServer().listen(0, '127.0.0.1');
  await once(server, 'listening');
  const number = server.address().port;
  await new Promise(done => server.close(done));
  return number;
}
async function start() {
  const child = spawn(process.execPath, [join(fixture, 'scripts/start-native.mjs'), '--skip-build'], { env, cwd: tmpdir(), stdio: ['ignore', 'pipe', 'pipe'] });
  current = { child, log: '', done: once(child, 'exit') };
  const instance = current;
  child.stdout.on('data', data => { instance.log += data; });
  child.stderr.on('data', data => { instance.log += data; });
  for (let i = 0; i < 600; i++) {
    if (instance.log.includes('API 与 Agent 已就绪')) return;
    if (child.exitCode !== null) throw Error(instance.log);
    await sleep(100);
  }
  throw Error(`Startup timed out: ${instance.log}`);
}
async function stopByFailure(external = false) {
  const pid = Number(current.log.match(/Agent PID (\d+)/)?.[1]);
  assert(pid, current.log);
  process.kill(pid); // Exercise failure propagation and cleanup on Windows as well as Unix.
  const [code] = await current.done;
  assert.equal(code, 1);
  assert(!existsSync(join(fixture, '.runtime/native/launcher.lock')));
  assert.equal(existsSync(join(fixture, '.runtime/native/postgres/postmaster.pid')), external);
  current = null;
}
try {
  for (const dir of ['scripts', 'server/target/release', 'client']) mkdirSync(join(fixture, dir), { recursive: true });
  for (const name of ['start-common.mjs', 'start-native.mjs']) copyFileSync(join(root, 'scripts', name), join(fixture, 'scripts', name));
  copyFileSync(join(root, '.env.example'), join(fixture, '.env.example'));
  copyFileSync(join(root, 'server/target/debug', executable), join(fixture, 'server/target/release', executable));
  cpSync(join(root, 'client/dist'), join(fixture, 'client/dist'), { recursive: true });
  const apiPort = await freePort(), agentPort = await freePort(), pgPort = await freePort();
  const configPath = join(fixture, '.env');
  createConfig(configPath, { ...defaults(), DATABASE_URL: '', PG_BIN: env.PG_BIN ?? '', MARIO_PORT: String(apiPort), MARIO_AGENT_PORT: String(agentPort), MARIO_PG_PORT: String(pgPort) });
  const pgBinFromEnvironment = env.PG_BIN;
  delete env.PG_BIN; // Verify the launcher can read the tool path entirely from the file.
  const original = readFileSync(configPath, 'utf8');
  assert.equal(createConfig(configPath, defaults()), false);
  assert.equal(readFileSync(configPath, 'utf8'), original);
  const config = readConfig(configPath);
  await start();
  assert(!current.log.includes(config.MARIO_MASTER_KEY));
  assert(!current.log.includes(config.POSTGRES_PASSWORD));
  const base = `http://127.0.0.1:${apiPort}`;
  assert.match(await (await fetch(base)).text(), /id="root"/);
  assert.equal((await fetch(`${base}/api/snapshot`)).status, 401);
  assert.equal((await fetch(`http://127.0.0.1:${agentPort}/health`)).status, 401);
  const email = 'launcher@example.test', password = 'launcher-password-123456';
  const registered = await fetch(`${base}/api/auth/register`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email, password, registrationKey: config.MARIO_REGISTRATION_KEY }) });
  assert.equal(registered.status, 200, await registered.text());
  const duplicate = spawnSync(process.execPath, [join(fixture, 'scripts/start-native.mjs'), '--skip-build'], { env, encoding: 'utf8', timeout: 10000 });
  assert.equal(duplicate.status, 1);
  assert.match(duplicate.stderr, /已有启动进程/);
  assert(existsSync(join(fixture, '.runtime/native/launcher.lock')));
  await stopByFailure();
  await start();
  assert.equal(readFileSync(configPath, 'utf8'), original);
  const login = await fetch(`${base}/api/auth/login`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email, password }) });
  assert.equal(login.status, 200, await login.text());
  await stopByFailure();
  // An external database skips all local PostgreSQL tools and is never stopped by the launcher.
  const data = join(fixture, '.runtime/native/postgres');
  const externalStart = spawnSync(actualPgCtl, ['-D', data, '-l', join(fixture, 'external-postgres.log'), '-w', '-t', '30', '-o', `-h 127.0.0.1 -p ${pgPort}`, 'start'], { stdio: 'ignore', timeout: 40000 });
  assert.equal(externalStart.status, 0);
  writeFileSync(configPath, original.replace('DATABASE_URL=\n', `DATABASE_URL=postgresql://mario:${encodeURIComponent(config.POSTGRES_PASSWORD)}@127.0.0.1:${pgPort}/postgres\n`));
  const pgBin = env.PG_BIN;
  env.PG_BIN = join(fixture, 'nonexistent-postgres-bin');
  await start();
  assert.equal((await fetch(`${base}/api/server`)).status, 200);
  await stopByFailure(true);
  if (pgBin) env.PG_BIN = pgBin; else delete env.PG_BIN;
  assert.equal(spawnSync(actualPgCtl, ['-D', data, '-w', '-m', 'fast', 'stop'], { stdio: 'ignore', timeout: 40000 }).status, 0);
  // Invalid ports must be rejected before any service is started.
  writeFileSync(configPath, original.replace(`MARIO_AGENT_PORT=${agentPort}`, `MARIO_AGENT_PORT=${apiPort}`));
  const invalid = spawnSync(process.execPath, [join(fixture, 'scripts/start-native.mjs'), '--skip-build'], { env, encoding: 'utf8', timeout: 10000 });
  assert.equal(invalid.status, 1);
  assert.match(invalid.stderr, /必须使用不同端口/);
  unlinkSync(configPath);
  const missingKeys = spawnSync(process.execPath, [join(fixture, 'scripts/start-native.mjs'), '--skip-build'], { env, encoding: 'utf8', timeout: 10000 });
  assert.equal(missingKeys.status, 1);
  assert.match(missingKeys.stderr, /请恢复原配置/);
  assert(!existsSync(configPath));
  // Legacy config migration must preserve every credential and keep a backup.
  writeFileSync(join(fixture, '.env.native'), original);
  await start();
  assert(!existsSync(join(fixture, '.env.native')));
  assert.equal(readFileSync(join(fixture, '.env.native.backup'), 'utf8'), original);
  assert.deepEqual(readConfig(configPath), config);
  await stopByFailure();
  if (pgBinFromEnvironment) env.PG_BIN = pgBinFromEnvironment;
  console.log('PASS native startup: authenticated PostgreSQL, web/API/Agent, registration, duplicate lock, failure cleanup, restart persistence, unchanged keys, external database lifecycle, port validation, missing-key protection');
} finally {
  if (current?.child.exitCode === null) {
    const pid = Number(current.log.match(/Agent PID (\d+)/)?.[1]);
    if (pid) { try { process.kill(pid); } catch {} await current.done; }
    else { current.child.kill(); await current.done; }
  }
  const data = join(fixture, '.runtime/native/postgres');
  if (existsSync(join(data, 'postmaster.pid'))) assert.equal(spawnSync(actualPgCtl, ['-D', data, '-w', '-m', 'fast', 'stop'], { stdio: 'ignore', timeout: 40000 }).status, 0);
  // Only remove the private directory made by mkdtemp for this run.
  assert(resolve(fixture).startsWith(resolve(tmpdir()) + sep + 'mario-start-'));
  rmSync(fixture, { recursive: true, force: true });
}
