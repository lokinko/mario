import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { domainConfig, domainSettings, setupDomain } from './setup-domain.mjs';
import { root } from './start-common.mjs';

const input = { domain: 'lokinko.cn', ip: '47.106.188.95' };
const bash = process.env.MARIO_TEST_BASH || 'bash';

function fixture(options = {}) {
  const directory = mkdtempSync(join(tmpdir(), 'mario-domain-'));
  const systemRoot = join(directory, 'system'), project = join(directory, 'project');
  const site = join(systemRoot, 'etc/nginx/sites-available/mario');
  const fragment = join(project, '.runtime/native/nginx.conf');
  mkdirSync(dirname(site), { recursive: true });
  mkdirSync(dirname(fragment), { recursive: true });
  writeFileSync(fragment, 'location ^~ /mario/ { proxy_pass http://127.0.0.1:4217/; }\n');
  if (options.previous !== false) writeFileSync(site, 'original site\n');
  let certificateReady = options.certificate !== false;
  const certificateDirectory = join(systemRoot, 'etc/letsencrypt/live/lokinko.cn');
  function certificate() {
    mkdirSync(certificateDirectory, { recursive: true });
    writeFileSync(join(certificateDirectory, 'fullchain.pem'), 'certificate');
    writeFileSync(join(certificateDirectory, 'privkey.pem'), 'key');
  }
  if (certificateReady) certificate();
  const calls = [], logs = [];
  let checkedFinal = false;
  const run = (name, argv, config = {}) => {
    calls.push({ name, argv, config });
    if (name === 'openssl' && argv.includes('-checkhost')) {
      const mismatch = options.mismatch && !calls.some(call => call.name === 'certbot' && call.argv[0] === 'certonly');
      return { ok: certificateReady, output: `Hostname ${argv.at(-1)} does ${mismatch ? 'NOT ' : ''}match certificate` };
    }
    if (name === 'openssl' && argv.includes('-checkend') && options.expired && !calls.some(call => call.name === 'certbot' && call.argv[0] === 'certonly'))
      return { ok: false, output: 'Certificate will expire' };
    if (name === 'nginx' && argv[0] === '-T')
      return { ok: true, output: `# configuration file ${options.conflict ? '/etc/nginx/sites-enabled/unrelated' : site}:\n    server_name lokinko.cn;\n` };
    if (name === 'nginx' && argv[0] === '-t' && existsSync(site) && readFileSync(site, 'utf8').includes('listen 443 ssl')) {
      checkedFinal = true;
      if (options.badConfig) throw Error('invalid Nginx configuration');
      if (options.warning) return { ok: true, output: 'conflicting server name "lokinko.cn"' };
    }
    if (name === 'certbot' && argv[0] === 'certonly') {
      if (options.checkUmask) assert.equal(process.umask(), 0o022, 'challenge files must be readable despite deploy.sh umask 0077');
      assert.match(readFileSync(site, 'utf8'), /location \^~ \/\.well-known\/acme-challenge\//);
      assert(!readFileSync(site, 'utf8').includes('listen 443 ssl'));
      assert(!readFileSync(site, 'utf8').includes('include '), 'HTTP bootstrap serves challenges only');
      if (options.certbotFailure) throw Error('HTTP challenge 403');
      certificateReady = true;
      certificate();
    }
    if (name === 'systemctl' && argv[0] === 'reload' && options.reloadFailure && checkedFinal) {
      checkedFinal = false;
      throw Error('reload failed');
    }
    return { ok: true, output: '' };
  };
  return {
    directory, systemRoot, project, site, calls, logs,
    setup: () => setupDomain(input, { systemRoot, project, run, log: line => logs.push(line) }),
    dispose: () => rmSync(directory, { recursive: true, force: true }),
  };
}

test('daily deploy and prepare skip domain setup; explicit domain + IP invokes exactly once', () => {
  const source = readFileSync(join(root, 'deploy.sh'), 'utf8');
  const configure = /^configure_domain\(\) \{[\s\S]*?^\}/m.exec(source)?.[0];
  assert(configure);
  // Exercise the actual shell guard with a recorder instead of root/system commands.
  for (const [action, domainProvided, ipProvided, count] of [
    ['deploy', 'false', 'false', 0], ['deploy', 'true', 'true', 1],
    ['prepare', 'true', 'true', 0], ['restart', 'true', 'true', 0],
  ]) {
    const result = spawnSync(bash, ['-euc', `${configure}\nNODE=record\nrecord() { printf '%s\\n' "$@"; }\nconfigure_domain`], {
      encoding: 'utf8', env: { ...process.env, ACTION: action, DOMAIN_PROVIDED: domainProvided, IP_PROVIDED: ipProvided,
        PROJECT_DIR: '/srv/mario', DOMAIN: input.domain, SERVER_IP: input.ip, SERVICE_NAME: 'mario', INSTALL_MISSING_TOOLS: 'true' },
    });
    assert.equal(result.status, 0, result.stderr);
    assert.equal((result.stdout.match(/setup-domain\.mjs/g) || []).length, count);
    if (count) for (const value of ['--domain', input.domain, '--ip', input.ip, '--install-tools']) assert(result.stdout.includes(value));
  }
  for (const argv of [['--domain', input.domain], ['--ip', input.ip]]) {
    const result = spawnSync(bash, [join(root, 'deploy.sh'), ...argv], { encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /同时提供/);
  }
});

test('configuration rejects injection and preserves requested canonical host, path and query', () => {
  for (const invalid of ['https://lokinko.cn/mario', 'lokinko.cn;return', 'bad..cn', 'localhost'])
    assert.throws(() => domainSettings({ ...input, domain: invalid }));
  assert.throws(() => domainSettings({ ...input, ip: 'not-an-ip' }));
  assert.throws(() => domainSettings({ ...input, prefix: '/mario' }));
  const settings = domainSettings({ ...input, domain: 'www.lokinko.cn', prefix: '/invest/' });
  assert.equal(settings.alias, 'lokinko.cn');
  const config = domainConfig(settings, { fragment: '/srv/mario/.runtime/native/nginx.conf', webroot: '/var/lib/mario-acme', certificate: '/cert/fullchain.pem', key: '/cert/privkey.pem' });
  assert.match(config, /https:\/\/www\.lokinko\.cn\$request_uri;/);
  assert.match(config, /return 308 \/invest\/\$is_args\$args;/);
  assert(!config.includes('\\;'));
  const rootSite = domainConfig(domainSettings({ ...input, prefix: '/' }), { fragment: '/fragment', webroot: '/acme' });
  assert(!rootSite.includes('location = /'));
});

test('valid dual-domain certificate is reused; existing site backed up; manual renewal reported', () => {
  const f = fixture();
  try {
    const renewal = join(f.systemRoot, 'etc/letsencrypt/renewal/lokinko.cn.conf');
    mkdirSync(dirname(renewal), { recursive: true });
    writeFileSync(renewal, 'authenticator = manual\n');
    f.setup();
    assert(!f.calls.some(call => call.name === 'certbot' && call.argv[0] === 'certonly'));
    const backups = join(f.systemRoot, 'var/backups/mario-nginx');
    assert.equal(readFileSync(join(backups, readdirSync(backups)[0]), 'utf8'), 'original site\n');
    assert.match(readFileSync(f.site, 'utf8'), /server_name www\.lokinko\.cn;/);
    assert(f.logs.some(line => line.includes('手动 DNS')));
    const stable = readFileSync(f.site, 'utf8');
    f.setup();
    assert.equal(readFileSync(f.site, 'utf8'), stable, 'repeated explicit setup preserves routing and reuses certificate');
    assert(!f.calls.some(call => call.name === 'certbot' && call.argv[0] === 'certonly'));
    assert(!existsSync(join(f.systemRoot, 'var/lock/mario-domain.lock')));
  } finally { f.dispose(); }
});

test('missing certificate bootstraps webroot validation then installs HTTPS and renewal hook', () => {
  const f = fixture({ certificate: false, previous: false });
  try {
    f.setup();
    const certbot = f.calls.find(call => call.name === 'certbot' && call.argv[0] === 'certonly');
    assert(certbot.argv.includes('--webroot'));
    assert(certbot.argv.includes('www.lokinko.cn'));
    assert(certbot.argv.includes('--deploy-hook'));
    assert(f.calls.findIndex(call => call.name === 'systemctl' && call.argv[0] === 'reload') < f.calls.indexOf(certbot));
    assert.match(readFileSync(f.site, 'utf8'), /listen 443 ssl/);
  } finally { f.dispose(); }
});

test('HTTP validation temporarily changes 0077 umask and restores it', { skip: process.platform === 'win32' }, () => {
  const f = fixture({ certificate: false, checkUmask: true });
  const originalUmask = process.umask(0o077);
  try {
    f.setup();
    assert.equal(process.umask(), 0o077);
  } finally { process.umask(originalUmask); f.dispose(); }
});

test('single-host certificate is rejected even when openssl hostname mismatch exits zero', () => {
  const f = fixture({ mismatch: true });
  try {
    f.setup();
    assert(f.calls.some(call => call.name === 'certbot' && call.argv[0] === 'certonly'));
  } finally { f.dispose(); }
});

test('expiring certificate is not silently reused', () => {
  const f = fixture({ expired: true });
  try {
    f.setup();
    assert(f.calls.some(call => call.name === 'certbot' && call.argv[0] === 'certonly'));
  } finally { f.dispose(); }
});

for (const failure of ['certbotFailure', 'badConfig', 'warning', 'reloadFailure']) {
  test(`${failure}: restore original site and reload; leave account data alone`, () => {
    const f = fixture({ [failure]: true, certificate: failure !== 'certbotFailure' });
    try {
      assert.throws(f.setup, /已恢复原 Nginx/);
      assert.equal(readFileSync(f.site, 'utf8'), 'original site\n');
      assert(f.calls.some(call => call.name === 'systemctl' && call.argv[0] === 'reload'));
      assert(!existsSync(join(f.systemRoot, 'var/lock/mario-domain.lock')));
      assert(!existsSync(join(f.project, '.env')));
      assert(!existsSync(join(f.project, '.runtime/native/postgres')));
    } finally { f.dispose(); }
  });
}

test('certificate failure on a fresh host removes only the newly written site', () => {
  const f = fixture({ certificate: false, previous: false, certbotFailure: true });
  try {
    assert.throws(f.setup, /已恢复原 Nginx/);
    assert(!existsSync(f.site));
  } finally { f.dispose(); }
});

test('other site ownership and concurrent setup are refused before changing the site', () => {
  const f = fixture({ conflict: true });
  try {
    assert.throws(f.setup, /其他站点/);
    assert.equal(readFileSync(f.site, 'utf8'), 'original site\n');
    const lock = join(f.systemRoot, 'var/lock/mario-domain.lock');
    writeFileSync(lock, 'existing');
    assert.throws(f.setup, /配置锁/);
    assert.equal(readFileSync(lock, 'utf8'), 'existing');
  } finally { f.dispose(); }
});
