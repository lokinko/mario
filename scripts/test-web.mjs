// Compiled Web assets served beside the authenticated API, against a DISPOSABLE PostgreSQL database.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { resolve } from 'node:path';
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
const binary = resolve(process.env.MARIO_TEST_SERVER_BIN ?? ('server/target/debug/mario-server' + (process.platform === 'win32' ? '.exe' : '')));
let child, log = '';
const origin = `http://127.0.0.1:${port}`;
async function start() {
  const env = { ...process.env, DATABASE_URL: database, MARIO_MASTER_KEY: masterKey, MARIO_WEB_DIR: resolve('client/dist'), MARIO_HOST: '127.0.0.1' };
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
  const asset = html.match(/src="([^\"]+\.js)"/)[1];
  const js = await (await fetch(origin + asset)).text();
  assert(js.length > 1000);
  assert(!js.includes(masterKey));
  const registered = await fetch(origin + '/api/auth/register', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ email: `${randomUUID()}@example.test`, password: 'web-password-0123456789' }) });
  assert.equal(registered.status, 200);
  const { token } = await registered.json();
  const headers = { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' };
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
  console.log('PASS Web: public assets, authenticated API, rejected wrong token, private files inaccessible, restart persistence');
} finally { await stop(); }
