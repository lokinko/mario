// End-to-end migration between two private, disposable PostgreSQL clusters.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { mkdtempSync, mkdirSync, copyFileSync, readFileSync, writeFileSync, existsSync, readdirSync, rmSync } from 'node:fs';
import { join, resolve, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as sleep } from 'node:timers/promises';
import { root, defaults, createConfig, readConfig, startPostgres } from './start-common.mjs';
import { command } from './backup-common.mjs';

const fixture = mkdtempSync(join(tmpdir(), 'mario-migration-'));
const pgBin = process.env.MARIO_TEST_PG_BIN;
const pg = name => pgBin ? join(pgBin, name + (process.platform === 'win32' ? '.exe' : '')) : name;
const passphrase = 'test-migration-password-123456';
const cleanEnv = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^(PG|MARIO_|DATABASE_URL$|POSTGRES_)/i.test(key)));
const sites = [], children = [];
async function freePort() {
  const server = createServer().listen(0, '127.0.0.1'); await once(server, 'listening');
  const value = server.address().port; await new Promise(done => server.close(done)); return value;
}
async function cluster(name, initialize = true) {
  const directory = join(fixture, name); mkdirSync(join(directory, 'scripts'), { recursive: true });
  for (const filename of ['package.json', '.env.example']) copyFileSync(join(root, filename), join(directory, filename));
  for (const filename of ['backup-common.mjs', 'backup.mjs', 'restore-backup.mjs', 'start-common.mjs']) copyFileSync(join(root, 'scripts', filename), join(directory, 'scripts', filename));
  const config = { ...defaults(), MARIO_DOMAIN: `${name}.example.test`, MARIO_PG_PORT: String(await freePort()), MARIO_PORT: String(await freePort()), PG_BIN: pgBin ?? '', MARIO_BACKUP_MODE: 'native' };
  createConfig(join(directory, '.env'), config);
  const data = join(directory, '.runtime/native/postgres'); mkdirSync(join(directory, '.runtime/native'), { recursive: true });
  const passwordFile = join(directory, 'initial-password'); writeFileSync(passwordFile, config.POSTGRES_PASSWORD, { mode: 0o600 });
  const site = { directory, config, data, uri: `postgresql://mario@127.0.0.1:${config.MARIO_PG_PORT}/postgres` };
  sites.push(site);
  if (!initialize) return site;
  await command(pg('initdb'), ['-D', data, '-U', 'mario', '--auth-host=scram-sha-256', '--auth-local=scram-sha-256', '--encoding=UTF8', '--no-locale', `--pwfile=${passwordFile}`], { env: cleanEnv });
  return site;
}
async function startDatabase(site) {
  await startPostgres(pg('pg_ctl'), site.data, join(site.directory, 'postgres.log'), site.config.MARIO_PG_PORT, { env: cleanEnv }, command);
}
async function stopDatabase(site) {
  if (existsSync(join(site.data, 'postmaster.pid'))) await command(pg('pg_ctl'), ['-D', site.data, '-w', '-t', '30', '-m', 'fast', 'stop'], { env: cleanEnv });
}
async function startApi(site) {
  const actual = readConfig(join(site.directory, '.env'));
  const binary = join(root, 'server/target/debug/mario-server' + (process.platform === 'win32' ? '.exe' : ''));
  const child = spawn(binary, ['--port', actual.MARIO_PORT], { env: { ...cleanEnv, DATABASE_URL: `postgresql://mario:${actual.POSTGRES_PASSWORD}@127.0.0.1:${actual.MARIO_PG_PORT}/postgres`, MARIO_HOST: '127.0.0.1', MARIO_MASTER_KEY: actual.MARIO_MASTER_KEY, MARIO_REGISTRATION_KEY: actual.MARIO_REGISTRATION_KEY }, stdio: ['ignore', 'pipe', 'pipe'] });
  let logs = ''; child.stdout.on('data', chunk => { logs += chunk; }); child.stderr.on('data', chunk => { logs += chunk; });
  children.push(child); site.child = child; site.base = `http://127.0.0.1:${actual.MARIO_PORT}/api`;
  for (let i = 0; i < 150; i++) {
    try { if ((await fetch(site.base + '/server')).ok) return; } catch {}
    if (child.exitCode !== null) throw Error(logs);
    await sleep(100);
  }
  throw Error('API startup timed out');
}
async function stopApi(site) {
  if (site.child?.exitCode === null && site.child.signalCode === null) { const done = once(site.child, 'exit'); site.child.kill(); await done; }
}
async function request(site, path, session, method = 'GET', body, expected = 200) {
  const response = await fetch(site.base + path, { method, headers: { 'Content-Type': 'application/json', ...(session ? { Authorization: `Bearer ${session.token}` } : {}), ...(session?.revision ? { 'If-Match': session.revision } : {}) }, body: body === undefined ? undefined : JSON.stringify(body) });
  const text = await response.text(); assert.equal(response.status, expected, `${path}: ${text}`);
  if (session && response.headers.has('x-data-revision')) session.revision = response.headers.get('x-data-revision');
  return JSON.parse(text);
}
const accountPassword = 'test-account-password-123';
async function register(site, email) {
  return request(site, '/auth/register', null, 'POST', { email, password: accountPassword, registrationKey: site.config.MARIO_REGISTRATION_KEY });
}
async function login(site, email) { return request(site, '/auth/login', null, 'POST', { email, password: accountPassword }); }
async function cli(site, name, argv, expected = 0, pwd = passphrase) {
  const result = spawnSync(process.execPath, [join(site.directory, 'scripts', name), ...argv], { cwd: tmpdir(), env: { ...cleanEnv, MARIO_BACKUP_PASSWORD: pwd }, encoding: 'utf8', timeout: 120000 });
  assert.equal(result.status, expected, result.stdout + result.stderr);
  assert(!(result.stdout + result.stderr).includes(site.config.MARIO_MASTER_KEY));
  return result;
}
async function sql(site, text) {
  return command(pg('psql'), ['--dbname', site.uri, '-X', '-tA', '-v', 'ON_ERROR_STOP=1', '-c', text], { env: { ...cleanEnv, PGPASSWORD: site.config.POSTGRES_PASSWORD }, capture: true });
}
try {
  const source = await cluster('source'), target = await cluster('target', false);
  await startDatabase(source); await startApi(source);
  const a = await register(source, 'a@example.test'), b = await register(source, 'b@example.test');
  let snapshot = await request(source, '/snapshot', a);
  await request(source, '/snapshot', b);
  await request(source, '/profile', a, 'PUT', { ...snapshot.profile, monthlyIncome: 45678 });
  await request(source, '/model-config', a, 'PUT', { provider: 'openai-responses', baseUrl: 'https://api.openai.com/v1', model: 'migration-test', apiKey: 'synthetic-provider-secret' });
  const first = join(fixture, 'first.mario-backup');
  await cli(source, 'backup.mjs', ['--output', first]); // Online consistent snapshot.
  const streamed = spawnSync(process.execPath, [join(source.directory, 'scripts/backup.mjs'), '--stdout', '--stdin-password'], { env: cleanEnv, input: passphrase + '\n', timeout: 120000 });
  assert.equal(streamed.status, 0, streamed.stderr.toString());
  assert.equal(streamed.stdout.subarray(0, 8).toString(), 'MARIOBK1'); // SSH channel contains only archive bytes.
  assert(!streamed.stdout.includes(source.config.MARIO_MASTER_KEY));
  const encrypted = readFileSync(first);
  assert(!encrypted.includes(source.config.MARIO_MASTER_KEY));
  assert(!encrypted.includes('synthetic-provider-secret'));
  await cli(source, 'backup.mjs', ['--output', first], 1);
  const targetEnv = readFileSync(join(target.directory, '.env'), 'utf8');
  await cli(target, 'restore-backup.mjs', ['--input', first], 1, 'incorrect-password-123456');
  assert.equal(readFileSync(join(target.directory, '.env'), 'utf8'), targetEnv);
  assert(!existsSync(join(target.data, 'postmaster.pid')));
  const broken = join(fixture, 'broken.mario-backup');
  const damaged = Buffer.from(encrypted); damaged[80] ^= 1; writeFileSync(broken, damaged);
  await cli(target, 'restore-backup.mjs', ['--input', broken], 1);
  await cli(target, 'restore-backup.mjs', ['--input', first]); // Empty target; temporary PG is stopped afterwards.
  assert(!existsSync(join(target.data, 'postmaster.pid')));
  const restoredConfig = readConfig(join(target.directory, '.env'));
  const restoredEnvText = readFileSync(join(target.directory, '.env'), 'utf8');
  assert.equal(restoredConfig.MARIO_MASTER_KEY, source.config.MARIO_MASTER_KEY);
  for (const key of ['MARIO_DOMAIN', 'POSTGRES_PASSWORD', 'MARIO_REGISTRATION_KEY', 'MARIO_AGENT_TOKEN', 'MARIO_PG_PORT']) assert.equal(restoredConfig[key], target.config[key]);
  await startDatabase(target); await startApi(target);
  await request(target, '/snapshot', a, 'GET', undefined, 401); // Source session revoked.
  const restored = await login(target, a.email);
  assert.equal((await request(target, '/snapshot', restored)).profile.monthlyIncome, 45678);
  assert.equal((await request(target, '/model-config', restored)).hasApiKey, true); // Proves the imported key decrypts.
  const extra = await register(target, 'target-only@example.test');
  const extraSnapshot = await request(target, '/snapshot', extra);
  await request(target, '/profile', extra, 'PUT', { ...extraSnapshot.profile, monthlyIncome: 90000 });
  await cli(target, 'restore-backup.mjs', ['--input', first, '--replace'], 1); // Refuses live API connections.
  await stopApi(target); await stopDatabase(target);
  await cli(target, 'restore-backup.mjs', ['--input', first], 1); // Refuses occupied target without explicit replacement.
  // A dump that depends on an excluded external type must fail inside the restore transaction.
  const schema = 'user_' + a.userId.replaceAll('-', '');
  await sql(source, `CREATE TYPE public.migration_guard AS ENUM ('test'); CREATE TABLE "${schema}".restore_guard (value public.migration_guard);`);
  const incompatible = join(fixture, 'incompatible.mario-backup');
  await cli(source, 'backup.mjs', ['--output', incompatible]);
  await cli(target, 'restore-backup.mjs', ['--input', incompatible, '--replace'], 1);
  assert.equal(readFileSync(join(target.directory, '.env'), 'utf8'), restoredEnvText);
  await startDatabase(target); await startApi(target);
  const survived = await login(target, extra.email);
  assert.equal((await request(target, '/snapshot', survived)).profile.monthlyIncome, 90000); // Drop/create rolled back.
  await stopApi(target); await stopDatabase(target);
  await sql(source, `DROP TABLE "${schema}".restore_guard; DROP TYPE public.migration_guard;`);
  snapshot = await request(source, '/snapshot', a);
  await request(source, '/profile', a, 'PUT', { ...snapshot.profile, monthlyIncome: 77777 });
  await stopApi(source); await stopDatabase(source);
  const second = join(fixture, 'second.mario-backup');
  await cli(source, 'backup.mjs', ['--output', second]); // Offline backup starts/stops only its own cluster.
  assert(!existsSync(join(source.data, 'postmaster.pid')));
  await cli(target, 'restore-backup.mjs', ['--input', second, '--replace']);
  await startDatabase(target); await startApi(target);
  const latest = await login(target, a.email);
  assert.equal((await request(target, '/snapshot', latest)).profile.monthlyIncome, 77777);
  await request(target, '/auth/login', null, 'POST', { email: extra.email, password: accountPassword }, 401);
  assert.equal(await sql(target, `SELECT count(*) FROM pg_namespace WHERE nspname='user_${extra.userId.replaceAll('-', '')}'`), '0');
  await stopApi(target); await stopDatabase(target);
  const safetyFiles = readdirSync(join(target.directory, 'backups')).filter(name => name.endsWith('.mario-backup')).sort();
  assert(safetyFiles.length >= 2);
  const safety = join(target.directory, 'backups', safetyFiles.at(-1));
  await cli(target, 'restore-backup.mjs', ['--input', safety, '--replace']);
  await startDatabase(target); await startApi(target);
  const recovered = await login(target, extra.email);
  assert.equal((await request(target, '/snapshot', recovered)).profile.monthlyIncome, 90000);
  console.log('PASS migration: encrypted online/offline snapshots, password/tamper checks, no overwrite, empty restore, all accounts, credential decryption, session revocation, destination config preservation, live API guard, SQL rollback, repeated replacement, orphan removal, safety backup recovery');
} finally {
  for (const site of sites) { await stopApi(site); await stopDatabase(site); }
  assert(resolve(fixture).startsWith(resolve(tmpdir()) + sep + 'mario-migration-'));
  rmSync(fixture, { recursive: true, force: true });
}
