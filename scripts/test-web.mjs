// Compiled Web assets served beside the authenticated API, against a DISPOSABLE PostgreSQL database.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { execFileSync } from 'node:child_process';
import { testPostgresBin } from './postgres-cluster.mjs';
import { once } from 'node:events';
import { join } from 'node:path';
import { root, serverBinary } from './server-artifacts.mjs';
import { randomBytes, randomUUID } from 'node:crypto';
import { createServer } from 'node:net';
import { setTimeout as sleep } from 'node:timers/promises';
const database = process.env.MARIO_TEST_DATABASE_URL;
assert(database, 'Set MARIO_TEST_DATABASE_URL to a disposable PostgreSQL database');
const probe = createServer().listen(0, '127.0.0.1');
await once(probe, 'listening');
const port = probe.address().port;
await new Promise(r => probe.close(r));
const masterKey = randomBytes(32).toString('base64');
const binary = serverBinary();
let child, log = '';
const origin = `http://127.0.0.1:${port}`;
async function start() {
  const env = { ...process.env, DATABASE_URL: database, MARIO_MASTER_KEY: masterKey, MARIO_WEB_DIR: join(root, 'client/dist'), MARIO_HOST: '127.0.0.1' };
  delete env.MARIO_AGENT_URL;
  child = spawn(binary, ['--port', String(port)], { env, stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout.on('data', b => log += b); child.stderr.on('data', b => log += b);
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch(origin)).ok) return; } catch {}
    if (child.exitCode !== null) throw Error(log);
    await sleep(50);
  }
  throw Error('Web server did not start: ' + log);
}
async function stop() { if (child?.exitCode === null) { const exit = once(child, 'exit'); child.kill(); await exit; } }
try {
  await start();
  const html = await (await fetch(origin)).text();
  assert.match(html, /<div id="root">/);
  for (const path of ['/dashboard', '/dashboard/']) {
    const page = await fetch(origin + path);
    assert.equal(page.status, 200);
    assert.match(await page.text(), /<div id="root">/);
  }
  const asset = html.match(/src="([^\"]+\.js)"/)[1];
  const js = await (await fetch(origin + asset)).text();
  assert(js.length > 1000);
  assert(!js.includes(masterKey));
  const registered = await fetch(origin + '/api/auth/register', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email: `${randomUUID()}@example.test`, password: 'web-password-0123456789' }) });
  assert.equal(registered.status, 200);
  const { token, userId } = await registered.json();
  const headers = { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' };
  assert.equal((await fetch(origin + '/api/dashboard')).status, 401);
  assert.equal((await fetch(origin + '/api/activity', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{"page":"watch"}' })).status, 401);
  assert.equal((await fetch(origin + '/api/dashboard', { headers })).status, 403);
  for (const body of [{ page: 'unknown' }, { page: 'watch', userId: 'other' }, { page: 'watch', timestamp: 0 }])
    assert.equal((await fetch(origin + '/api/activity', { method: 'POST', headers, body: JSON.stringify(body) })).status, 422);
  const beforeActivity = await fetch(origin + '/api/snapshot', { headers });
  const revisionBeforeActivity = beforeActivity.headers.get('x-data-revision');
  for (let i = 0; i < 2; i++) assert.equal((await fetch(origin + '/api/activity', { method: 'POST', headers, body: '{"page":"watch"}' })).status, 200);
  assert.equal((await fetch(origin + '/api/snapshot', { headers })).headers.get('x-data-revision'), revisionBeforeActivity);
  const psql = join(testPostgresBin(), 'psql' + (process.platform === 'win32' ? '.exe' : ''));
  assert.match(userId, /^[a-f0-9-]+$/);
  const role = (enabled) => execFileSync(psql, ['-X', '--dbname', database, '--set=ON_ERROR_STOP=1', '-c', `UPDATE mario_auth.users SET is_superuser=${enabled} WHERE id='${userId}'`], { stdio: 'pipe', windowsHide: true });
  role(true);
  const boardResponse = await fetch(origin + '/api/dashboard?days=7', { headers });
  assert.equal(boardResponse.status, 200);
  assert.equal(boardResponse.headers.get('cache-control'), 'no-store');
  const board = await boardResponse.json();
  assert.equal(board.days, 7);
  assert(board.dailyActive >= 1);
  assert([1, 2].includes(board.users.find(user => user.userId === userId).activeMinutes));
  assert.equal((await fetch(origin + '/api/dashboard?days=8', { headers })).status, 400);
  role(false);
  assert.equal((await fetch(origin + '/api/dashboard', { headers })).status, 403);
  assert.equal((await fetch(origin + '/api/snapshot')).status, 401);
  assert.equal((await fetch(origin + '/api/snapshot', { headers: { Authorization: 'Bearer invalid' } })).status, 401);
  assert.equal((await fetch(origin + '/api/not-a-route', { headers })).status, 404);
  for (const path of ['/.env', '/.runtime/dev/master-key', '/web-access-key']) assert.equal((await fetch(origin + path)).status, 404);
  const response = await fetch(origin + '/api/snapshot', { headers });
  const initial = await response.json();
  const profile = { ...initial.profile, monthlyIncome: 12345 };
  const saved = await fetch(origin + '/api/profile', { method: 'PUT', headers: { ...headers, 'If-Match': response.headers.get('x-data-revision') }, body: JSON.stringify(profile) });
  assert.equal(saved.status, 200, await saved.text());
  await stop(); await start();
  const restored = await (await fetch(origin + '/api/snapshot', { headers })).json();
  assert.equal(restored.profile.monthlyIncome, 12345);
  role(true);
  const restoredBoard = await (await fetch(origin + '/api/dashboard?days=7', { headers })).json();
  assert.equal(restoredBoard.users.find(user => user.userId === userId).activeMinutes,
    board.users.find(user => user.userId === userId).activeMinutes);
  await fetch(origin + '/api/auth/logout', { method: 'POST', headers });
  assert.equal((await fetch(origin + '/api/dashboard', { headers })).status, 401);
  assert.equal((await fetch(origin + '/api/activity', { method: 'POST', headers, body: '{"page":"watch"}' })).status, 401);
  console.log('PASS dashboard: direct paths, admin authorization/revocation, authenticated sampling, deduplication, unchanged ledger revision and restart persistence');
  console.log('PASS Web: public assets, authenticated API, rejected wrong token, private files inaccessible, restart persistence');
} finally { await stop(); }
