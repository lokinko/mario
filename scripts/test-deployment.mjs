// Public deployment smoke test: read-only; creates no accounts or business data.
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { args, basePath, readConfig, root } from './start-common.mjs';

try {
  const options = args({ '--help': false, '--url': true });
  if (options['--help']) {
    console.log('用法：node scripts/test-deployment.mjs --url https://www.madeagents.ai/mario\n只读检查 HTTPS、API 部署模式、网页资源和未登录访问保护，不创建账号/数据，也不调用模型。');
  } else {
    const config = existsSync(join(root, '.env')) ? readConfig(join(root, '.env')) : {};
    const url = new URL(options['--url'] || `https://${config.MARIO_DOMAIN || 'localhost'}${basePath(config.MARIO_BASE_PATH)}`);
    if (url.username || url.password || url.search || url.hash) throw Error('--url 填写网站入口，可带部署路径，不带账号或查询参数');
    const prefix = basePath(url.pathname.endsWith('/') ? url.pathname : `${url.pathname}/`);
    url.pathname = prefix;
    if (url.protocol !== 'https:' && !(url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname))) throw Error('公网验收请使用 HTTPS；本机 HTTP 仅支持回环地址');
    const origin = url.origin;
    let checks = 0;
    async function request(path, statuses = [200]) {
      const response = await fetch(new URL(path, url), { redirect: 'manual', signal: AbortSignal.timeout(15000) });
      if (response.status >= 300 && response.status < 400) throw Error(`${path} 返回 ${response.status} 跳转至 ${response.headers.get('location')}；请检查 Nginx 的 www→根域名跳转`);
      assert(statuses.includes(response.status), `${path}: HTTP ${response.status}，预期 ${statuses.join('/')}`);
      checks++;
      return response;
    }
    if (prefix !== '/') {
      const response = await fetch(origin + prefix.slice(0, -1), { redirect: 'manual', signal: AbortSignal.timeout(15000) });
      assert([301, 302, 307, 308].includes(response.status), '无尾斜杠入口应跳转至同站点带尾斜杠入口');
      assert.equal(new URL(response.headers.get('location'), origin).href, url.href, '入口跳转路径错误，可能仍在跳转到根域名');
      checks++;
    }
    const server = await (await request('api/server')).json();
    assert.equal(server.mode, 'hosted');
    assert.equal(server.database, 'postgresql');
    assert.equal(server.protocol, 1);
    console.log(`PASS ${url.protocol === 'https:' ? 'HTTPS' : '本机 HTTP'} 与多用户 PostgreSQL API`);
    const page = await (await request('')).text();
    assert.match(page, /id=["']root["']/);
    const script = page.match(/<script\b[^>]*\bsrc=["']([^"']+)["']/i)?.[1];
    assert(script, '网页缺少脚本资源');
    const asset = new URL(script, url);
    assert.equal(asset.origin, origin, '当前网页脚本应使用同源资源');
    assert(asset.pathname.startsWith(prefix), '网页脚本未包含部署前缀，请重新构建前端');
    const javascript = await (await request(asset.pathname + asset.search)).text();
    assert(javascript.length > 1000, '网页脚本资源不完整');
    const stylesheet = page.match(/<link\b(?=[^>]*\brel=["']stylesheet["'])[^>]*\bhref=["']([^"']+)["']/i)?.[1];
    assert(stylesheet, '网页缺少 CSS 资源');
    const css = new URL(stylesheet, url);
    assert(css.origin === origin && css.pathname.startsWith(prefix), 'CSS 未包含部署前缀');
    assert.match((await request(css.pathname)).headers.get('content-type') || '', /text\/css/);
    assert.match(await (await request('mario-mark.svg')).text(), /<svg/);
    console.log('PASS 网页与 JavaScript 资源');
    await request('api/snapshot', [401]);
    await request('api/model-config', [401]);
    await request('.env', [403, 404]);
    await request('mario.db', [403, 404]);
    console.log(`PASS 未登录访问保护与私有文件保护；共 ${checks} 项只读 HTTP 检查通过。`);
    console.log('下一步：浏览器注册两个账号，验证数据隔离、跨设备同步及一个 AI 问答。');
  }
} catch (error) {
  console.error(`部署验收失败：${error.message}`);
  process.exitCode = 1;
}
