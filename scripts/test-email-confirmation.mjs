import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';

const project = process.argv[2];
assert.match(project || '', /^[a-z0-9]{20}$/);
const callback = 'https://lokinko.github.io/mario/auth/';
const keys = JSON.parse(execFileSync('supabase', ['projects', 'api-keys', '--project-ref', project, '--output', 'json'], { encoding: 'utf8' }));
const admin = keys.find(k => k.name === 'service_role').api_key;
const publicKey = keys.find(k => k.type === 'publishable').api_key;
const base = `https://${project}.supabase.co/auth/v1`;
function request(path, body, key = admin, method = 'POST', bearer = key) {
  const args = ['-4', '--max-time', '30', '-fsS', '-X', method, '-H', `apikey: ${key}`, '-H', `Authorization: Bearer ${bearer}`, '-H', 'Content-Type: application/json'];
  if (body) args.push('--data-binary', '@-');
  args.push(`${base}${path}`);
  return JSON.parse(execFileSync('curl', args, { input: body ? JSON.stringify(body) : undefined, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }));
}
function follow(link) {
  assert.equal(new URL(link).origin, new URL(base).origin);
  const headers = execFileSync('curl', ['-4', '--max-time', '30', '-sS', '-D', '-', '-o', '/dev/null', link], { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] });
  const status = [...headers.matchAll(/^HTTP\/\S+ (\d{3})/gm)].at(-1)?.[1];
  console.log(`Verification endpoint HTTP ${status || 'unknown'}`);
  const location = headers.match(/^location: (.+)$/im)?.[1].trim();
  assert.ok(location, 'Verification must redirect');
  const url = new URL(location);
  assert.equal(url.origin + url.pathname, callback);
  return new URLSearchParams(url.hash.slice(1));
}
let userId;
let stage = 'create temporary account';
try {
  const email = `mario-confirm-${randomUUID()}@example.com`;
  const password = `${randomUUID()}Aa1!`;
  const user = request('/admin/users', { email, password, email_confirm: false });
  userId = user.id;
  assert.ok(userId);
  assert.ok(!user.email_confirmed_at);
  for (const type of ['signup', 'recovery']) {
    stage = `${type}: generate link`;
    const generated = request('/admin/generate_link', { type, email, password });
    console.log(`Checking ${type} callback routing.`);
    const link = generated.action_link || generated.properties?.action_link;
    assert.equal(new URL(link).searchParams.get('redirect_to'), callback);
    stage = `${type}: verify and check redirect`;
    const fragment = follow(link);
    assert.equal(fragment.get('type'), type);
    assert.ok(fragment.get('access_token'));
    stage = `${type}: read confirmed user`;
    const verified = request('/user', null, publicKey, 'GET', fragment.get('access_token'));
    assert.equal(verified.id, userId);
    assert.ok(verified.email_confirmed_at);
    if (type === 'recovery') {
      stage = 'recovery: update password';
      request('/user', { password: `${randomUUID()}Bb2!` }, publicKey, 'PUT', fragment.get('access_token'));
    }
    stage = `${type}: reject reused link`;
    const reused = follow(link);
    console.log(`${type} reused-link result: error=${Boolean(reused.get('error'))}, accessToken=${Boolean(reused.get('access_token'))}`);
    assert.ok(reused.get('error'), 'Used links must fail at the public callback');
    assert.ok(!reused.get('access_token'));
  }
  console.log('Email callback E2E passed: unconfirmed account, signup verification, public redirect, confirmed user, browser recovery API, reused-link rejection.');
} catch {
  // Never print curl arguments or provider responses containing test credentials.
  console.error(`Email callback E2E failed at ${stage}; inspect configuration and connectivity. No credentials logged.`);
  process.exitCode = 1;
} finally {
  if (userId) {
    try { request(`/admin/users/${userId}`, null, admin, 'DELETE'); }
    catch { console.error('Temporary confirmation account cleanup failed.'); process.exitCode = 1; }
  }
}
