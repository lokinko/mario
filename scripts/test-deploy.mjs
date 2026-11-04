// Exercise the single deployment entry without touching system packages/services.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readlinkSync, renameSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { join, resolve, sep } from 'node:path';
import { tmpdir, userInfo } from 'node:os';
import { createConfig, defaults, readConfig, root, validateSecrets } from './start-common.mjs';

const fixture = mkdtempSync(join(tmpdir(), 'mario-deploy-'));
const cleanEnv = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^(MARIO_|DATABASE_URL$|PG_BIN$|HF_ENDPOINT$)/i.test(key)));
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
  for (const name of ['host-runtime.mjs', 'start-common.mjs', 'benchmark-deploy.mjs']) copyFileSync(join(root, 'scripts', name), join(fixture, 'scripts', name));
  copyFileSync(join(root, '.env.example'), join(fixture, '.env.example'));
  copyFileSync(join(root, 'deploy.sh'), join(fixture, 'deploy.sh'));
  createConfig(envPath, defaults('old.example.test'));
  const before = readConfig(envPath);
  assert.equal(Object.hasOwn(before, 'MARIO_REGISTRATION_KEY'), false);
  assert.doesNotThrow(() => validateSecrets(before));
  assert.doesNotThrow(() => validateSecrets({ ...before, MARIO_REGISTRATION_KEY: 'obsolete' }));
  const options = { MARIO_SETUP_DOMAIN: 'www.madeagents.ai', MARIO_SETUP_IP: '47.243.99.21', MARIO_SETUP_BASE_PATH: '/mario/', MARIO_SETUP_API_PORT: '4321', MARIO_SETUP_BENCH_ENABLED: 'true' };
  const result = prepare({ ...options, HF_ENDPOINT: 'https://hf-mirror.com' });
  const after = readConfig(envPath);
  for (const key of ['MARIO_MASTER_KEY', 'POSTGRES_PASSWORD', 'MARIO_AGENT_TOKEN']) {
    assert.equal(after[key], before[key]);
    assert(!(result.stdout + result.stderr).includes(after[key]));
  }
  assert.equal(after.MARIO_DOMAIN, 'www.madeagents.ai');
  assert.equal(after.MARIO_SERVER_IP, '47.243.99.21');
  assert.equal(after.MARIO_BASE_PATH, '/mario/');
  assert.equal(after.MARIO_BACKUP_MODE, 'native');
  assert.equal(after.MARIO_BENCH_ENABLED, 'true');
  assert.equal(after.HF_ENDPOINT, 'https://hf-mirror.com');
  prepare({ ...options, HF_ENDPOINT: '' });
  assert.equal(readConfig(envPath).HF_ENDPOINT, '');
  prepare({ ...options, HF_ENDPOINT: after.HF_ENDPOINT });
  const nginx = readFileSync(join(fixture, '.runtime/native/nginx.conf'), 'utf8');
  assert.match(nginx, /location = \/mario \{ return 308 \/mario\/\$is_args\$args;/);
  assert.match(nginx, /proxy_pass http:\/\/127\.0\.0\.1:4321\//);
  assert.match(nginx, /proxy_set_header Host \$host/);
  assert.match(nginx, /Content-Security-Policy/);
  const callback = /location = \/mario\/api\/auth\/openai\/callback \{([^}]+)\}/.exec(nginx)?.[1];
  assert(callback, 'OAuth callback must bypass the SPA location');
  assert.match(callback, /access_log off/);
  assert.match(callback, /proxy_pass http:\/\/127\.0\.0\.1:4321\/api\/auth\/openai\/callback;/);
  const stable = readFileSync(envPath, 'utf8');
  prepare(options);
  assert.equal(readFileSync(envPath, 'utf8'), stable);
  for (const invalid of [
    { MARIO_SETUP_IP: 'not-an-ip' }, { MARIO_SETUP_DOMAIN: 'https://bad.example.test' },
    { MARIO_SETUP_BASE_PATH: '/mario' }, { MARIO_SETUP_API_PORT: '4218', MARIO_SETUP_AGENT_PORT: '4218' },
    { MARIO_SETUP_BENCH_ENABLED: 'invalid' },
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
  const deployScript = readFileSync(join(fixture, 'deploy.sh'), 'utf8');
  const homeCheck = /^check_user_home\(\) \{[\s\S]*?^\}/m.exec(deployScript)?.[0];
  const homePrepare = /^prepare_user_home\(\) \{[\s\S]*?^\}/m.exec(deployScript)?.[0];
  assert(homeCheck, 'deploy must check the service user home before installing tools');
  assert(homePrepare, 'deploy must repair service user home and tool permissions');
  const home = join(fixture, 'user-home');
  mkdirSync(home);
  // Root CI must exercise permission checks as an unprivileged user too.
  const rootTest = process.getuid?.() === 0;
  const testUser = rootTest ? 'nobody' : userInfo().username;
  chmodSync(fixture, 0o755);
  chmodSync(home, 0o777);
  function runHome(command, expected, path = '') {
    const switchUser = rootTest ? 'runuser -u "$RUN_USER" -- ' : '';
    const script = `${homePrepare}\n${homeCheck}\ncd "$TEST_ROOT"\nRUN_HOME="$PWD/user-home"\nRUN_USER="$TEST_RUN_USER"\nas_user() { ${switchUser}env HOME="$RUN_HOME" USER="$RUN_USER" "$@"; }\n${command}`;
    const result = spawnSync(bash, ['-euc', script], {
      encoding: 'utf8', env: { ...cleanEnv, TEST_ROOT: fixture, TEST_RUN_USER: testUser },
    });
    assert.equal(result.status, expected, result.stdout + result.stderr);
    if (expected !== 0) assert(result.stderr.includes(path), result.stderr);
    return result;
  }
  function checkHome(expected, path = '') {
    const result = runHome('check_user_home', expected, path);
    if (expected !== 0) {
      assert.match(result.stderr, /namei -l/);
      assert.match(result.stderr, /docs\/deploy-madeagents.md/);
    }
  }
  checkHome(0); // Missing tool directories are created later by their installers.
  if (process.platform !== 'win32') {
    chmodSync(home, 0o500);
    checkHome(1, 'user-home');
    chmodSync(home, 0o777);
  }
  for (const path of ['.cargo', '.cargo/bin', '.rustup', '.local', '.local/bin']) {
    const directory = join(home, path);
    writeFileSync(directory, 'not a directory');
    checkHome(1, path);
    rmSync(directory);
    mkdirSync(directory);
    chmodSync(directory, 0o777);
    if (process.platform !== 'win32') {
      chmodSync(directory, 0o500);
      checkHome(1, path);
      chmodSync(directory, 0o777);
    }
    checkHome(0);
  }
  if (process.platform !== 'win32') {
    if (rootTest) assert.equal(spawnSync('chown', [testUser, home]).status, 0);
    const rustup = join(home, '.cargo/bin/rustup');
    writeFileSync(rustup, 'old');
    chmodSync(rustup, 0o500);
    symlinkSync('rustup', join(home, '.cargo/bin/cargo'));
    const unrelated = join(home, 'keep.txt');
    writeFileSync(unrelated, 'keep');
    chmodSync(unrelated, 0o400);
    const localData = join(home, '.local/share');
    mkdirSync(localData);
    chmodSync(localData, 0o500);
    const external = join(fixture, 'external.txt');
    writeFileSync(external, 'outside tool directories');
    chmodSync(external, 0o400);
    symlinkSync(external, join(home, '.cargo/external'));
    const unchanged = [unrelated, localData, external].map(path => {
      const { uid, gid, mode } = statSync(path);
      return { path, uid, gid, mode };
    });
    for (const path of [home, join(home, '.cargo'), join(home, '.cargo/bin'), join(home, '.rustup'), join(home, '.local/bin')]) chmodSync(path, 0o500);
    runHome('prepare_user_home\nas_user sh -eu -c \'printf fixed >> "$HOME/.cargo/bin/rustup"\'', 0);
    assert.equal(readFileSync(rustup, 'utf8'), 'oldfixed');
    assert.equal(statSync(rustup).mode & 0o777, 0o700, 'repair preserves executable bits');
    assert.equal(readlinkSync(join(home, '.cargo/bin/cargo')), 'rustup');
    for (const before of unchanged) {
      const { uid, gid, mode } = statSync(before.path);
      assert.deepEqual({ uid, gid, mode }, { uid: before.uid, gid: before.gid, mode: before.mode });
    }
    runHome('prepare_user_home', 0); // Repeated deploys keep toolchains and unrelated data.
    renameSync(join(home, '.cargo'), join(home, '.cargo.saved'));
    symlinkSync(localData, join(home, '.cargo'));
    runHome('prepare_user_home', 1, '.cargo');
    assert.equal(readlinkSync(join(home, '.cargo')), localData);
    rmSync(join(home, '.cargo'));
    renameSync(join(home, '.cargo.saved'), join(home, '.cargo'));
    renameSync(home, join(fixture, 'saved-home'));
    runHome('prepare_user_home', 0); // Existing account without a home is repaired too.
    if (rootTest) {
      assert.equal(spawnSync('chown', ['0', home]).status, 0);
      runHome('prepare_user_home', 1, '非默认 home');
      assert.equal(statSync(home).uid, 0, 'custom root-owned home is not taken over');
    }
    console.log('PASS deploy permissions: automatic home/tool repair, service-user write, executable links, idempotence, scoped ownership, missing home, linked-directory rejection');
  } else {
    console.log('SKIP deploy permission repair: Linux/macOS ownership semantics required; Windows only exercises prepare and permission diagnostics');
  }
  for (const argv of [['-n', join(fixture, 'deploy.sh')], [join(fixture, 'deploy.sh'), '--help']])
    assert.equal(spawnSync(bash, argv, { encoding: 'utf8', env: cleanEnv }).status, 0);
  const shell = spawnSync(bash, [join(fixture, 'deploy.sh'), 'prepare', '--ip', '47.243.99.21', '--domain', 'www.madeagents.ai'], { encoding: 'utf8', env: cleanEnv });
  assert.equal(shell.status, 0, shell.stdout + shell.stderr);
  assert.equal(readConfig(envPath).MARIO_MASTER_KEY, before.MARIO_MASTER_KEY);
  const final = readFileSync(envPath, 'utf8');
  assert.equal(spawnSync(bash, [join(fixture, 'deploy.sh'), 'prepare', '--domain'], { encoding: 'utf8', env: cleanEnv }).status, 1);
  assert.equal(readFileSync(envPath, 'utf8'), final);
  assert(!existsSync(join(fixture, '.runtime/native/postgres/PG_VERSION')));
  console.log('PASS deploy entry: service user home/tool permissions, Bash syntax/help/prepare, persistent secrets, idempotent configuration, IP/domain/prefix/port validation, restore lock, generated Nginx route and headers');
} finally {
  assert(resolve(fixture).startsWith(resolve(tmpdir()) + sep + 'mario-deploy-'));
  rmSync(fixture, { recursive: true, force: true });
}
