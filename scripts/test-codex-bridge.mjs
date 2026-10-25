// Disposable end-to-end test against MARIO_TEST_DATABASE_URL; --live explicitly uses local Codex quota.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { randomBytes, randomUUID } from 'node:crypto';
import { mkdtemp, rm, writeFile, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join, sep } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
const live = process.argv.includes('--live');
const protocolFixture = process.argv.includes('--protocol-fixture');
const webSearch = protocolFixture || process.argv.includes('--web-search');
assert(!(live && protocolFixture), '--live and --protocol-fixture are mutually exclusive');
assert(process.env.MARIO_TEST_DATABASE_URL, 'Set MARIO_TEST_DATABASE_URL to a disposable PostgreSQL database (npm run test:codex-bridge starts one)');
const binary = resolve(process.env.MARIO_TEST_SERVER_BIN || `server/target/debug/mario-server${process.platform === 'win32' ? '.exe' : ''}`);
const directory = await mkdtemp(join(tmpdir(), 'mario-bridge-test-'));
const probe = createServer().listen(0, '127.0.0.1'); await once(probe, 'listening');
const port = probe.address().port; await new Promise(done => probe.close(done));
const site = `http://127.0.0.1:${port}`;
let auth;
const revisions = new Map();
const environment = { ...process.env, MARIO_HOST: '127.0.0.1', DATABASE_URL: process.env.MARIO_TEST_DATABASE_URL,
  MARIO_MASTER_KEY: randomBytes(32).toString('base64'),
  // Bridge must bypass the independent HTTP Agent, which cannot access this process's relay.
  MARIO_AGENT_URL: 'http://127.0.0.1:1', MARIO_AGENT_TOKEN: 'unused-test-token' };
for (const key of ['MARIO_WEB_DIR', 'MARIO_ROLE']) delete environment[key];
const children = [];
let logs = '', running = true, workerFailure;
function start(args) {
  const child = spawn(binary, args, { env: environment, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
  child.stdout.on('data', data => { logs += data; });
  child.stderr.on('data', data => { logs += data; });
  children.push(child); return child;
}
async function request(path, method = 'GET', body, token = auth, expected = 200) {
  const response = await fetch(`${site}/api${path}`, { method, headers: { ...(token ? { Authorization: `Bearer ${token}` } : {}), 'Content-Type': 'application/json', ...(revisions.has(token) ? {'If-Match':revisions.get(token)} : {}) }, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(210000) });
  const data = await response.json();
  assert.equal(response.status, expected, `${path}: ${JSON.stringify(data)}`);
  if (response.ok && response.headers.has('x-data-revision')) revisions.set(token, response.headers.get('x-data-revision'));
  return data;
}
const report = JSON.stringify({ answer:'请先明确资金用途', facts:[{statement:'用户提出本轮问题',basis:'user_data',evidenceIds:[],sourceRefs:['/question']}], suggestions:[{factIndices:[0],action:'先保持现有资金安排',why:'当前没有投资资料',unlessWhen:'明确资金用途后再调整'}], question:'这笔钱什么时候要用？' });
let fakeTask;
let holdNext = false, heldJob, cancelledJob;
let expectedModel = 'bridge-fixture';
try {
  if (protocolFixture) {
    const fixture=join(directory,`codex-fixture${process.platform==='win32'?'.exe':''}`);
    execFileSync('rustc',[resolve('scripts/fixtures/codex-app-server.rs'),'--edition','2021','-o',fixture],{stdio:'pipe'});
    environment.MARIO_CODEX_BIN=fixture;
    environment.MARIO_TEST_CODEX_SCENARIO=join(directory,'scenario');
    environment.MARIO_TEST_CODEX_COUNTER=join(directory,'counter');
    environment.MARIO_TEST_CODEX_REPORT=join(directory,'report');
    await writeFile(environment.MARIO_TEST_CODEX_SCENARIO,'success');
    await writeFile(environment.MARIO_TEST_CODEX_REPORT,report);
  }
  let server = start(['--port', String(port)]);
  for (let attempt = 0; attempt < 100; attempt++) {
    try { await request('/server'); break; } catch { if (server.exitCode !== null) throw Error('服务启动失败'); await sleep(100); }
  }
  const register = () => request('/auth/register', 'POST', {email:`${randomUUID()}@example.test`,password:'bridge-disposable-test-password'});
  auth = (await register()).token;
  const other = (await register()).token;
  await request('/snapshot');
  await request('/snapshot', 'GET', undefined, other);
  const { pairingCode } = await request('/codex-bridge', 'POST');
  await request('/codex-bridge/worker/poll', 'POST', { runner:randomUUID(), model:'fixture' }, auth, 401);
  await request('/model-config', 'GET', undefined, pairingCode, 401);
  await request('/codex-bridge/worker/poll', 'POST', { runner:randomUUID(), model:'retired-worker' }, pairingCode, 400);
  assert.equal((await request('/codex-bridge')).online, false);
  await request('/codex-bridge/enable', 'POST', undefined, auth, 400);
  if (live || protocolFixture) {
    const worker = start(['--codex-bridge', site]);
    worker.stdin.end(pairingCode + '\n');
  } else {
    const runner = randomUUID();
    fakeTask = (async () => {
      while (running) {
        const job = await request('/codex-bridge/worker/poll', 'POST', { runner, model:'bridge-fixture', models:[
          {model:'bridge-fixture',displayName:'Fixture Default',description:'Default model',isDefault:true},
          {model:'bridge-alternative',displayName:'Fixture Alternative',description:'Alternate model',isDefault:false}
        ] }, pairingCode);
        if (!job) continue;
        assert.equal(job.model, expectedModel, '推理必须使用网页已保存的模型');
        const event = event => request('/codex-bridge/worker/event', 'POST', { runner, jobId: job.id, event }, pairingCode);
        if (holdNext) {
          holdNext = false;
          heldJob = job.id;
          for (let attempt = 0; attempt < 100; attempt++) {
            const state = await event({type:'heartbeat'});
            if (!state.active) { cancelledJob = job.id; break; }
            await sleep(100);
          }
          continue;
        }
        await event({ type:'delta', text:report.slice(0, 80) });
        await sleep(50);
        await event({ type:'delta', text:report.slice(80) });
        await event({ type:'complete', result: { content:report, api_calls:1, usage:{input_tokens:10,output_tokens:5}, sources:[], search_calls:0, warnings:[] } });
      }
    })().catch(error => { if (running) workerFailure = error; });
  }
  let status;
  for (let attempt = 0; attempt < 100; attempt++) {
    status = await request('/codex-bridge'); if (status.online) break;
    if (workerFailure) throw workerFailure;
    await sleep(500);
  }
  assert(status.online, '本机桥接没有上线');
  let config = await request('/codex-bridge/enable', 'POST');
  assert.equal(config.provider, 'codex-bridge');
  assert.equal(config.model, status.model);
  assert(status.models.some(entry => entry.model === status.model), '缺少本机模型列表');
  if (!live) {
    assert.equal(status.models.length,2);
    await request('/model-config', 'PUT', {provider:'codex-bridge',baseUrl:'codex://bridge',model:'not-advertised'},auth,400);
    expectedModel = 'bridge-alternative';
    config = await request('/model-config', 'PUT', {provider:'codex-bridge',baseUrl:'codex://bridge',model:expectedModel});
    assert.equal((await request('/model-config')).model,expectedModel);
    assert.equal((await request('/codex-bridge/enable','POST')).model,expectedModel,'再次启用不得重置为默认模型');
    console.log('PASS 动态模型列表、非默认模型保存、非法模型拒绝');
  }
  assert.equal((await request('/codex-bridge', 'GET', undefined, other)).online, false);
  await request('/codex-bridge/enable', 'POST', undefined, other, 400);
  await request('/model-config', 'PUT', {provider:'codex-bridge',baseUrl:'codex://bridge',model:config.model}, other, 400);
  const otherPair = await request('/codex-bridge', 'POST', undefined, other);
  assert.notEqual(otherPair.pairingCode, pairingCode);
  await request('/codex-bridge', 'DELETE', undefined, other);
  assert.equal((await request('/codex-bridge')).online, true);
  console.log('PASS PostgreSQL 多账号配对隔离、不能借用其他账号桥接、撤销互不影响');
  const connection = await request('/model-config/test', 'POST');
  assert.equal(connection.ok, true);
  console.log(`PASS ${live ? '真实 Codex' : '模拟 Codex'} 配对、启用、模型连接；模型 ${config.model}`);
  if (protocolFixture) {
    await writeFile(environment.MARIO_TEST_CODEX_SCENARIO,'transient');
    await writeFile(environment.MARIO_TEST_CODEX_COUNTER,'0');
  }
  const input = { question:'这是连通性验收。当前没有提供任何投资资料，请简短说明还需要哪些资料；不要提供交易建议。', workflow:'quick', webSearch };
  const preview = await request('/analysis/preview', 'POST', input);
  const response = await fetch(`${site}/api/analysis/stream`, { method:'POST', headers:{Authorization:`Bearer ${auth}`,'Content-Type':'application/json',...(revisions.has(auth)?{'If-Match':revisions.get(auth)}:{})}, body:JSON.stringify({...input,previewRevision:preview.contextRevision}), signal:AbortSignal.timeout(280000) });
  assert.equal(response.status, 200);
  const events = (await response.text()).split('\n').filter(line => line.startsWith('data: ')).map(line => JSON.parse(line.slice(6)));
  assert(events.some(event => event.type === 'draft'), '缺少流式草稿');
  const completed = events.find(event => event.type === 'complete');
  assert(completed, `没有完成事件: ${JSON.stringify(events.filter(e => e.type === 'error'))}`);
  assert.equal(completed.result.transparency.provider, 'codex-bridge');
  assert.equal((await request('/analyses')).length, 1);
  if (protocolFixture) {
    assert(events.some(event=>event.type==='status' && event.message.includes('自动重试')),'重试进度必须经过桥接到达网页');
    assert.equal(completed.result.workflowTrace.calls.length,2,'必须执行搜索和单次分析');
    assert.equal(completed.result.workflowTrace.calls[0].requestCount,2);
    console.log('PASS Codex App Server 503 自动恢复 → 桥接重试状态 → 单次分析+联网 → 单次保存');
    for (const [scenario,expected] of [['quota','用量限制'],['partial','暂时不可用'],['native-retry',null]]) {
      await writeFile(environment.MARIO_TEST_CODEX_SCENARIO,scenario);
      await writeFile(environment.MARIO_TEST_CODEX_COUNTER,'0');
      const result=await request('/model-config/test','POST',undefined,auth,expected?502:200);
      if (expected) assert(result.error.includes(expected),JSON.stringify(result));
      else assert(result.ok);
      assert.equal(await readFile(environment.MARIO_TEST_CODEX_COUNTER,'utf8'),'1',`${scenario} 不应启动额外推理`);
      assert(!JSON.stringify(result).includes('SECRET-test-provider-body'));
    }
    assert.equal((await request('/analyses')).length,1);
    assert(!logs.includes('SECRET-test-provider-body'));
    console.log('PASS 额度不重试、已有输出不重放、原生恢复不叠加重试、错误分类保留且不泄露原文');
  }
  console.log('PASS 预览 → 桥接推理 → SSE 草稿 → 完整结果 → 历史保存；独立 Agent 已绕过');
  if (!live && !protocolFixture) {
    const nextPreview = await request('/analysis/preview', 'POST', input);
    holdNext = true;
    const abort = new AbortController();
    const pending = await fetch(`${site}/api/analysis/stream`, {method:'POST',headers:{Authorization:`Bearer ${auth}`,'Content-Type':'application/json',...(revisions.has(auth)?{'If-Match':revisions.get(auth)}:{})},body:JSON.stringify({...input,previewRevision:nextPreview.contextRevision}),signal:abort.signal});
    const reading = pending.text().catch(() => 'cancelled');
    for (let attempt=0;attempt<100 && !heldJob;attempt++) await sleep(50);
    assert(heldJob, '未收到待取消请求');
    abort.abort(); await reading;
    for (let attempt=0;attempt<100 && !cancelledJob;attempt++) await sleep(100);
    assert.equal(cancelledJob, heldJob, '浏览器停止必须传递到本机工作进程');
    assert.equal((await request('/analyses')).length,1,'取消结果不得写入历史');
    console.log('PASS 浏览器中止传递到工作进程，取消回答不保存');
  }
  running = false;
  await request('/codex-bridge', 'DELETE');
  assert.equal((await request('/codex-bridge')).online, false);
  assert.equal((await request('/model-config')).hasApiKey, false);
  await request('/codex-bridge/worker/event', 'POST', {runner:randomUUID(),jobId:randomUUID(),event:{type:'heartbeat'}}, pairingCode, 401);
  assert(!logs.includes(pairingCode), '配对码不得出现在日志');
  console.log('PASS 撤销、凭证边界与配对码日志检查');
  const replacement = await request('/codex-bridge', 'POST');
  server.kill(); await once(server, 'exit');
  server = start(['--port', String(port)]);
  for (let attempt=0;attempt<100;attempt++) {
    try { await request('/server'); break; } catch { await sleep(100); }
  }
  assert.equal((await request('/codex-bridge')).paired,false);
  await request('/codex-bridge/worker/poll','POST',{runner:randomUUID(),model:'fixture'},replacement.pairingCode,401);
  console.log('PASS 服务器重启使旧配对失效，需要重新配对');
} catch (error) {
  console.error(error.message);
  // Only our own sanitized process diagnostics; no auth files or raw RPC output.
  console.error(logs.slice(-4000));
  process.exitCode = 1;
} finally {
  running = false;
  for (const child of children.reverse()) {
    if (child.exitCode === null) { child.kill(); await Promise.race([once(child, 'exit'), sleep(5000)]); }
  }
  await fakeTask;
  assert(resolve(directory).startsWith(resolve(tmpdir()) + sep + 'mario-bridge-test-'));
  await rm(directory, { recursive:true, force:true });
}
