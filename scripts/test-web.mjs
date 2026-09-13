import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { randomBytes } from 'node:crypto';
import { createServer } from 'node:net';
import { setTimeout as sleep } from 'node:timers/promises';
const dir = await mkdtemp(join(tmpdir(), 'mario-web-test-'));
const probe = createServer().listen(0, '127.0.0.1');
await once(probe, 'listening');
const port = probe.address().port;
await new Promise(r => probe.close(r));
const token = randomBytes(32).toString('hex');
const binary = resolve('server/target/debug/mario-server' + (process.platform === 'win32' ? '.exe' : ''));
let child, log = '';
const origin = `http://127.0.0.1:${port}`;
const headers = { Authorization: `Bearer ${token}`, 'Content-Type':'application/json' };
async function start() {
  child = spawn(binary, ['--port', String(port)], {env:{...process.env, MARIO_DATA_DIR:dir, MARIO_AUTH_TOKEN:token, MARIO_WEB_DIR:resolve('client/dist'), MARIO_HOST:'127.0.0.1'}, stdio:['ignore','pipe','pipe']});
  child.stdout.on('data', b => log += b); child.stderr.on('data', b => log += b);
  for(let i=0; i<100; i++) {
    try { if((await fetch(origin)).ok) return; } catch {}
    if(child.exitCode !== null) throw Error(log);
    await sleep(50);
  }
  throw Error('Web server did not start: '+log);
}
async function stop() { if(child?.exitCode === null) { const exit = once(child,'exit'); child.kill(); await exit; } }
try {
  await start();
  const html = await (await fetch(origin)).text();
  assert.match(html, /<div id="root">/);
  const asset = html.match(/src="([^\"]+\.js)"/)[1];
  const js = await (await fetch(origin+asset)).text();
  assert(js.length > 1000);
  assert(!js.includes(token));
  assert.equal((await fetch(origin+'/api/snapshot')).status,401);
  assert.equal((await fetch(origin+'/api/snapshot',{headers:{Authorization:'Bearer invalid'}})).status,401);
  assert.equal((await fetch(origin+'/api/not-a-route',{headers})).status,404);
  assert.equal((await fetch(origin+'/mario.db')).status,404);
  assert.equal((await fetch(origin+'/web-access-key')).status,404);
  const initial = await (await fetch(origin+'/api/snapshot',{headers})).json();
  const profile = {...initial.profile, monthlyIncome:12345};
  assert.equal((await fetch(origin+'/api/profile',{method:'PUT',headers,body:JSON.stringify(profile)})).status,200);
  await stop(); await start();
  const restored = await (await fetch(origin+'/api/snapshot',{headers})).json();
  assert.equal(restored.profile.monthlyIncome,12345);
  console.log('PASS Web: public assets, authenticated API, rejected wrong key, private files inaccessible, restart persistence');
} finally { await stop(); await rm(dir,{recursive:true,force:true}); }
