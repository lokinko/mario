// Explicit, one-time host configuration. Daily deployments never call this entry.
import { spawnSync } from 'node:child_process';
import { chmodSync, closeSync, existsSync, lstatSync, mkdirSync, openSync, readFileSync, readdirSync, readlinkSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { isIP } from 'node:net';
import { dirname, join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { args, basePath, readConfig, root } from './start-common.mjs';

function command(name, argv, { capture = true, optional = false } = {}) {
  const result = spawnSync(name, argv, { encoding: 'utf8', stdio: capture ? 'pipe' : 'inherit' });
  const output = (result.stdout || '') + (result.stderr || '');
  if (!optional && (result.error || result.status !== 0))
    throw Error(`${name} 执行失败：${result.error?.message || output || result.status}`);
  return { ok: !result.error && result.status === 0, output };
}

export function domainSettings({ domain, ip, prefix = '/mario/', service = 'mario' }) {
  domain = String(domain || '').toLowerCase();
  if (domain.length > 249 || !/^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$/.test(domain))
    throw Error('域名须为完整主机名，不包含协议、端口、路径或通配符');
  if (!isIP(ip || '')) throw Error('必须提供有效公网 IP');
  if (!/^[a-zA-Z0-9_-]+$/.test(service)) throw Error('服务名格式无效');
  // Keep the requested canonical host; support its www/non-www counterpart.
  const alias = domain.startsWith('www.') ? domain.slice(4) : `www.${domain}`;
  return { domain, alias, ip, prefix: basePath(prefix), service };
}

const quote = value => {
  if (/["$;{}\r\n]/.test(value)) throw Error('Nginx 路径含不支持的字符');
  return `"${value.replaceAll('\\', '/')}"`;
};

export function domainConfig(settings, { fragment, webroot, certificate, key }) {
  const { domain, alias, prefix } = settings;
  const challenge = `    location ^~ /.well-known/acme-challenge/ {\n        root ${quote(webroot)};\n    }\n`;
  const route = `    include ${quote(fragment)};\n` + (prefix === '/' ? '' :
    `    location = / { return 308 ${prefix}$is_args$args; }\n    location / { return 404; }\n`);
  const header = '# Mario 域名接入，由 scripts/setup-domain.mjs 管理。\n';
  if (!certificate) return `${header}server {\n    listen 80;\n    server_name ${domain} ${alias};\n${challenge}    location / { return 404; }\n}\n`;
  const tls = `    ssl_certificate ${quote(certificate)};\n    ssl_certificate_key ${quote(key)};\n    ssl_protocols TLSv1.2 TLSv1.3;\n`;
  return `${header}server {\n    listen 80;\n    server_name ${domain} ${alias};\n${challenge}    location / { return 308 https://${domain}$request_uri; }\n}\n\n` +
    `server {\n    listen 443 ssl;\n    server_name ${alias};\n${tls}    return 308 https://${domain}$request_uri;\n}\n\n` +
    `server {\n    listen 443 ssl;\n    server_name ${domain};\n${tls}${route}}\n`;
}

function atomicWrite(path, text, mode = 0o644) {
  const temporary = `${path}.${randomUUID()}.tmp`;
  try {
    writeFileSync(temporary, text, { mode, flag: 'wx' });
    renameSync(temporary, path);
  } finally { if (existsSync(temporary)) unlinkSync(temporary); }
}

// systemRoot/run/log isolate system effects in regression tests; CLI always uses the real host.
export function setupDomain(input, { project = root, systemRoot = '/', run = command, log = console.log } = {}) {
  const settings = domainSettings(input);
  const { domain, alias, service } = settings;
  const system = path => join(systemRoot, path);
  const fragment = join(project, '.runtime/native/nginx.conf');
  if (!existsSync(fragment)) throw Error('缺少 Nginx 片段，请先用 deploy.sh prepare 或完成部署');
  quote(fragment);
  const available = system('etc/nginx/sites-available');
  const enabled = system('etc/nginx/sites-enabled');
  const site = join(available, service);
  const link = join(enabled, service);
  const webroot = system(`var/lib/${service}-acme`);
  const live = system('etc/letsencrypt/live');
  const lock = system(`var/lock/${service}-domain.lock`);
  mkdirSync(dirname(lock), { recursive: true });
  let lockFd;
  try { lockFd = openSync(lock, 'wx', 0o600); }
  catch { throw Error(`域名配置锁已存在或无法创建：${lock}；请确认没有配置进程后再处理`); }
  // deploy.sh uses 0077 for application secrets. HTTP challenge files must be readable by Nginx workers.
  const previousUmask = process.umask(0o022);
  try {
    const tools = [['nginx', ['-v']], ['openssl', ['version']], ['certbot', ['--version']]];
    const missing = tools.filter(([name, argv]) => !run(name, argv, { optional: true }).ok).map(([name]) => name);
    if (missing.length) {
      if (!input.installTools) throw Error(`请先安装：${missing.join('、')}`);
      run('apt-get', ['update'], { capture: false });
      run('apt-get', ['install', '-y', ...missing], { capture: false });
    }
    const check = () => {
      const result = run('nginx', ['-t']);
      if (/conflicting server name/i.test(result.output)) throw Error(result.output);
    };
    check();
    const dump = run('nginx', ['-T']).output;
    let file = '';
    for (const line of dump.split('\n')) {
      if (line.startsWith('# configuration file ')) file = line.slice(21).replace(/:\s*$/, '');
      const names = /^\s*server_name\s+([^;]+);/.exec(line)?.[1].split(/\s+/) || [];
      if (names.some(name => [domain, alias].includes(name)) && ![site, link].includes(file))
        throw Error(`域名已由其他站点管理：${file}；请先合并该站点，未覆盖原配置`);
    }
    for (const directory of [available, enabled, webroot]) mkdirSync(directory, { recursive: true, mode: 0o755 });
    chmodSync(webroot, 0o755);
    const stat = path => { try { return lstatSync(path); } catch (error) { if (error.code !== 'ENOENT') throw error; } };
    const oldSite = stat(site);
    if (oldSite && !oldSite.isFile()) throw Error(`站点文件必须是普通文件：${site}`);
    const oldLink = stat(link);
    if (oldLink && (!oldLink.isSymbolicLink() || resolve(dirname(link), readlinkSync(link)) !== resolve(site)))
      throw Error(`启用入口已被其他配置占用：${link}`);
    const previous = oldSite ? readFileSync(site) : undefined;
    if (previous) {
      const backups = system(`var/backups/${service}-nginx`);
      mkdirSync(backups, { recursive: true, mode: 0o700 });
      const backup = join(backups, `${Date.now()}-${randomUUID()}.conf`);
      writeFileSync(backup, previous, { mode: 0o600, flag: 'wx' });
      log(`原站点已备份：${backup}`);
    }
    const reload = () => {
      check();
      run('systemctl', ['enable', '--now', 'nginx'], { capture: false });
      run('systemctl', ['reload', 'nginx'], { capture: false });
    };
    const valid = directory => {
      const certificate = join(directory, 'fullchain.pem'), key = join(directory, 'privkey.pem');
      if (!existsSync(certificate) || !existsSync(key)) return false;
      return [domain, alias].every(host => {
        const result = run('openssl', ['x509', '-in', certificate, '-noout', '-checkhost', host], { optional: true });
        // Some OpenSSL releases report a hostname mismatch with exit status 0.
        return result.ok && result.output.includes('does match certificate');
      }) &&
        run('openssl', ['x509', '-in', certificate, '-noout', '-checkend', '86400'], { optional: true }).ok;
    };
    let certificateDirectory = [domain, ...(existsSync(live) ? readdirSync(live).filter(name => name !== domain) : [])]
      .map(name => join(live, name)).find(valid);
    try {
      atomicWrite(site, domainConfig(settings, { fragment, webroot, certificate: certificateDirectory && join(certificateDirectory, 'fullchain.pem'), key: certificateDirectory && join(certificateDirectory, 'privkey.pem') }));
      if (!oldLink) {
        run('ln', ['-s', site, link]);
      }
      reload();
      if (!certificateDirectory) {
        log(`请先将 ${domain} 和 ${alias} 解析到 ${settings.ip}，放通公网 80/443；现开始 HTTP 证书验证。`);
        run('certbot', ['certonly', '--webroot', '--preferred-challenges', 'http-01', '-w', webroot, '--cert-name', domain,
          '-d', domain, '-d', alias, '--deploy-hook', 'nginx -t && systemctl reload nginx'], { capture: false });
        certificateDirectory = join(live, domain);
        if (!valid(certificateDirectory)) throw Error('申请后的证书未覆盖两个域名或即将过期');
        atomicWrite(site, domainConfig(settings, { fragment, webroot, certificate: join(certificateDirectory, 'fullchain.pem'), key: join(certificateDirectory, 'privkey.pem') }));
        reload();
      }
    } catch (error) {
      if (previous !== undefined) atomicWrite(site, previous, oldSite.mode & 0o777);
      else if (existsSync(site)) unlinkSync(site);
      if (!oldLink && stat(link)) unlinkSync(link);
      try { reload(); }
      catch (rollback) { throw Error(`${error.message}\n配置文件已恢复，但重新加载失败：${rollback.message}`); }
      throw Error(`${error.message}\n已恢复原 Nginx 站点配置；若 HTTP 验证被 403 拦截，需先处理公网接入或自行用 DNS 验证申请双域名证书后重试。`);
    }
    const renewal = system(`etc/letsencrypt/renewal/${certificateDirectory.split(/[\\/]/).at(-1)}.conf`);
    const renewalConfig = existsSync(renewal) ? readFileSync(renewal, 'utf8') : '';
    if (/^authenticator\s*=\s*manual\s*$/m.test(renewalConfig) && !/^manual_auth_hook\s*=\s*\S/m.test(renewalConfig))
      log('复用了手动 DNS 证书：当前脚本未配置 DNS 自动验证，续期仍需原手动流程或另行配置认证 hook。');
    log(`域名接入完成：${domain}、${alias} → https://${domain}${settings.prefix}；DNS、备案与公网访问仍需 deploy.sh check 验收。`);
  } finally {
    process.umask(previousUmask);
    closeSync(lockFd);
    unlinkSync(lock);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = args({ '--help': false, '--domain': true, '--ip': true, '--service': true, '--install-tools': false });
    if (options['--help']) console.log('用法：sudo node scripts/setup-domain.mjs --domain lokinko.cn --ip 47.106.188.95 [--service mario] [--install-tools]\n通常由 deploy.sh 显式收到 --domain 和 --ip 后调用；日常部署不调用。复用双域名证书，否则用 HTTP 验证申请。');
    else {
      if (process.platform !== 'linux' || process.getuid?.() !== 0) throw Error('域名配置需要在 Linux 上以 root 运行');
      const config = readConfig(join(root, '.env'));
      setupDomain({ domain: options['--domain'], ip: options['--ip'], service: options['--service'] || 'mario', prefix: config.MARIO_BASE_PATH, installTools: !!options['--install-tools'] });
    }
  } catch (error) { console.error(`域名接入失败：${error.message}`); process.exitCode = 1; }
}
