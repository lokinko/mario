// PostgreSQL clusters owned by tests and `npm run dev`. They never read .env or DATABASE_URL.
// CLI: node scripts/postgres-cluster.mjs <command> [args...] runs the command with
// MARIO_TEST_DATABASE_URL pointing at a fresh disposable database, then deletes the cluster.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { resolve, join, dirname, basename } from 'node:path';
import { tmpdir } from 'node:os';
import { randomBytes } from 'node:crypto';
import { createServer } from 'node:net';
import { fileURLToPath } from 'node:url';
import { postgresBin, postgresStartOptions } from './start-common.mjs';

export const root = resolve(import.meta.dirname, '..');
const suffix = process.platform === 'win32' ? '.exe' : '';

export function testPostgresBin() {
  const configured = process.env.MARIO_TEST_PG_BIN || postgresBin();
  if (configured) return resolve(configured);
  const probe = spawnSync('pg_config', ['--bindir'], { encoding: 'utf8' });
  const found = probe.status === 0 ? probe.stdout.trim() : '';
  assert(found && existsSync(join(found, 'initdb' + suffix)), '需要本机 PostgreSQL：安装后确保 pg_config 在 PATH 中，或设置 MARIO_TEST_PG_BIN 为 PostgreSQL bin 目录');
  return found;
}

export function run(command, args, env = {}, capture = false) {
  return new Promise((accept, reject) => {
    const childEnv = { ...process.env, ...env };
    delete childEnv.PGSERVICE; delete childEnv.PGSERVICEFILE; delete childEnv.PGOPTIONS;
    const child = spawn(command, args, { cwd: root, env: childEnv, windowsHide: true, stdio: capture ? ['ignore', 'pipe', 'pipe'] : 'inherit' });
    let output = '', error = '';
    child.stdout?.on('data', chunk => output += chunk);
    child.stderr?.on('data', chunk => error += chunk);
    child.once('error', reject);
    child.once('exit', code => code === 0 ? accept(output) : reject(Error(`${basename(command)} failed (${code})\n${error.slice(-4000)}`)));
  });
}

export async function freePort() {
  const socket = createServer();
  await new Promise(r => socket.listen(0, '127.0.0.1', r));
  const { port } = socket.address();
  await new Promise(r => socket.close(r));
  return port;
}

// A started cluster: `url(name)` builds a connection string, `createDatabase` is idempotent.
async function cluster({ data, port, password, log }) {
  const bin = testPostgresBin();
  const pg = name => join(bin, name + suffix);
  const pgEnv = { PGHOST: '127.0.0.1', PGPORT: String(port), PGUSER: 'mario_test', PGPASSWORD: password, PGDATABASE: 'postgres', PGSERVICE: '', PGSERVICEFILE: '', PGOPTIONS: '' };
  if (!existsSync(join(data, 'PG_VERSION'))) {
    const passwordFile = join(dirname(data), 'password');
    await writeFile(passwordFile, password, { mode: 0o600 });
    await run(pg('initdb'), ['-D', data, '-U', 'mario_test', '-A', 'scram-sha-256', '--pwfile', passwordFile, '--encoding=UTF8', '--no-locale'], {}, true);
  }
  if (spawnSync(pg('pg_ctl'), ['-D', data, 'status'], { stdio: 'ignore' }).status !== 0)
    await run(pg('pg_ctl'), ['-D', data, '-l', log, '-o', postgresStartOptions(port), '-w', 'start'], {}, true);
  const url = name => `postgresql://mario_test:${password}@127.0.0.1:${port}/${name}`;
  return {
    pg, pgEnv, url,
    async createDatabase(name) {
      assert.match(name, /^[a-z_][a-z0-9_]*$/);
      const exists = await run(pg('psql'), ['-X', '-At', '-c', `SELECT 1 FROM pg_database WHERE datname='${name}'`], pgEnv, true);
      if (!exists.trim()) await run(pg('psql'), ['-X', '--set=ON_ERROR_STOP=1', '-c', `CREATE DATABASE ${name}`], pgEnv, true);
      return url(name);
    },
    stop: () => run(pg('pg_ctl'), ['-D', data, '-m', 'fast', '-w', 'stop'], {}, true),
  };
}

export async function withDisposableCluster(prefix, work) {
  assert.match(prefix, /^mario-[a-z-]+-$/);
  const directory = await mkdtemp(join(tmpdir(), prefix));
  let started;
  try {
    started = await cluster({ data: join(directory, 'data'), port: await freePort(), password: randomBytes(24).toString('hex'), log: join(directory, 'postgres.log') });
    return await work({ ...started, directory });
  } finally {
    if (started) await started.stop();
    assert.equal(dirname(resolve(directory)), resolve(tmpdir())); assert(basename(directory).startsWith(prefix));
    await rm(directory, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 });
  }
}

// Persistent development cluster under .runtime/dev; reused across `npm run dev` sessions.
export async function developmentCluster(port) {
  const directory = resolve(root, '.runtime/dev');
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const secret = async (name, create) => {
    const path = join(directory, name);
    if (!existsSync(path)) await writeFile(path, create(), { mode: 0o600 });
    return (await readFile(path, 'utf8')).trim();
  };
  const password = await secret('password', () => randomBytes(24).toString('hex'));
  const masterKey = await secret('master-key', () => randomBytes(32).toString('base64'));
  const started = await cluster({ data: join(directory, 'postgres'), port, password, log: join(directory, 'postgres.log') });
  return { ...started, masterKey };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...args] = process.argv.slice(2);
  assert(command, 'usage: node scripts/postgres-cluster.mjs <command> [args...]');
  await withDisposableCluster('mario-test-', async ({ createDatabase }) => {
    await run(command, args, { MARIO_TEST_DATABASE_URL: await createDatabase('mario_test') });
  });
}
