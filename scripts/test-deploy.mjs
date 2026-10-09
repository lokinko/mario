// Exercise the single deployment entry without touching system packages/services.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { createConfig, defaults, readConfig, root } from './start-common.mjs';

const fixture = mkdtempSync(join(tmpdir(), 'mario-deploy-'));
const cleanEnv = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^(MARIO_|DATABASE_URL$|PG_BIN$)/i.test(key)));
// Git Bash otherwise rewrites /mario/ into a Windows filesystem path for Node.
if (process.platform === 'win32') cleanEnv.MSYS2_ENV_CONV_EXCL = 'MARIO_SETUP_BASE_PATH';
const envPath = join(fixture, '.env');
function prepare(overrides, expected = 0) {
  const result = spawnSync(process.execPath, [join(fixture, 'scripts/host-runtime.mjs'), '--prepare'], {
    env: { ...cleanEnv, ...overrides }, encoding: 'utf8', timeout: 15000,
  });
  assert.equal(result.status, expected, result.stdout + result.stderr);
  return result;
}
try {
  mkdirSync(join(fixture, 'scripts'));
  for (const name of ['host-runtime.mjs', 'start-common.mjs']) copyFileSync(join(root, 'scripts', name), join(fixture, 'scripts', name));
  copyFileSync(join(root, '.env.example'), join(fixture, '.env.example'));
  copyFileSync(join(root, 'deploy.sh'), join(fixture, 'deploy.sh'));
  createConfig(envPath, defaults('old.example.test'));
  const before = readConfig(envPath);
  const options = { MARIO_SETUP_DOMAIN: 'www.madeagents.ai', MARIO_SETUP_IP: '47.243.99.21', MARIO_SETUP_BASE_PATH: '/mario/', MARIO_SETUP_API_PORT: '4321' };
  const result = prepare(options);
  const after = readConfig(envPath);
  for (const key of ['MARIO_MASTER_KEY', 'POSTGRES_PASSWORD', 'MARIO_REGISTRATION_KEY', 'MARIO_AGENT_TOKEN']) {
    assert.equal(after[key], before[key]);
    assert(!(result.stdout + result.stderr).includes(after[key]));
  }
  assert.equal(after.MARIO_DOMAIN, 'www.madeagents.ai');
  assert.equal(after.MARIO_SERVER_IP, '47.243.99.21');
  assert.equal(after.MARIO_BASE_PATH, '/mario/');
  assert.equal(after.MARIO_BACKUP_MODE, 'native');
  const nginx = readFileSync(join(fixture, '.runtime/native/nginx.conf'), 'utf8');
  assert.match(nginx, /location = \/mario \{ return 308 \/mario\/\$is_args\$args;/);
  assert.match(nginx, /proxy_pass http:\/\/127\.0\.0\.1:4321\//);
  assert.match(nginx, /proxy_set_header Host \$host/);
  assert.match(nginx, /Content-Security-Policy/);
  const stable = readFileSync(envPath, 'utf8');
  prepare(options);
  assert.equal(readFileSync(envPath, 'utf8'), stable);
  for (const invalid of [
    { MARIO_SETUP_IP: 'not-an-ip' }, { MARIO_SETUP_DOMAIN: 'https://bad.example.test' },
    { MARIO_SETUP_BASE_PATH: '/mario' }, { MARIO_SETUP_API_PORT: '4218', MARIO_SETUP_AGENT_PORT: '4218' },
  ]) {
    prepare({ ...options, ...invalid }, 1);
    assert.equal(readFileSync(envPath, 'utf8'), stable);
  }
  writeFileSync(join(fixture, '.runtime/restore.lock'), '1');
  prepare(options, 1);
  assert.equal(readFileSync(envPath, 'utf8'), stable);
  rmSync(join(fixture, '.runtime/restore.lock'));
  renameSync(envPath, join(fixture, '.env.native'));
  prepare({});
  assert.equal(readConfig(envPath).MARIO_DOMAIN, 'www.madeagents.ai');
  assert.equal(readConfig(envPath).MARIO_MASTER_KEY, before.MARIO_MASTER_KEY);
  assert(existsSync(join(fixture, '.env.native.backup')));
  const bash = process.env.MARIO_TEST_BASH || 'bash';
  const probe = spawnSync(bash, ['--version'], { encoding: 'utf8' });
  if (probe.error) throw Error('脚本测试需要 Bash；Windows 可通过 MARIO_TEST_BASH 指定 Git Bash 路径');
  for (const argv of [['-n', join(fixture, 'deploy.sh')], [join(fixture, 'deploy.sh'), '--help']])
    assert.equal(spawnSync(bash, argv, { encoding: 'utf8', env: cleanEnv }).status, 0);
  const shell = spawnSync(bash, [join(fixture, 'deploy.sh'), 'prepare', '--ip', '47.243.99.21', '--domain', 'www.madeagents.ai'], { encoding: 'utf8', env: cleanEnv });
  assert.equal(shell.status, 0, shell.stdout + shell.stderr);
  assert.equal(readConfig(envPath).MARIO_MASTER_KEY, before.MARIO_MASTER_KEY);
  const final = readFileSync(envPath, 'utf8');
  assert.equal(spawnSync(bash, [join(fixture, 'deploy.sh'), 'prepare', '--domain'], { encoding: 'utf8', env: cleanEnv }).status, 1);
  assert.equal(readFileSync(envPath, 'utf8'), final);
  assert(!existsSync(join(fixture, '.runtime/native/postgres/PG_VERSION')));
  console.log('PASS deploy entry: Bash syntax/help/prepare, persistent secrets, idempotent configuration, IP/domain/prefix/port validation, restore lock, generated Nginx route and headers');
} finally {
  assert(resolve(fixture).startsWith(resolve(tmpdir()) + sep + 'mario-deploy-'));
  rmSync(fixture, { recursive: true, force: true });
}
