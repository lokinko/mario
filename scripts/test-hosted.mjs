// Run against a DISPOSABLE PostgreSQL database. Never use a production URL.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {once} from 'node:events';
import {createServer} from 'node:net';
import {createServer as httpServer} from 'node:http';
import {randomBytes,randomUUID} from 'node:crypto';
import {resolve} from 'node:path';
import {setTimeout as sleep} from 'node:timers/promises';
const database=process.env.MARIO_TEST_DATABASE_URL;
assert(database,'Set MARIO_TEST_DATABASE_URL to a disposable PostgreSQL database');
const binary=resolve('server/target/debug/mario-server'+(process.platform==='win32'?'.exe':''));
const master=randomBytes(32).toString('base64'), invite=randomBytes(32).toString('hex');
const children=[]; let logs=''; let assertions=0;
const agentToken=randomBytes(32).toString('hex');
let receivedJob, releaseAgent, receivedResolve;
const received=new Promise(resolve=>receivedResolve=resolve);
const mockAgent=httpServer(async(req,res)=>{
  if(req.headers.authorization!==`Bearer ${agentToken}`){res.writeHead(401);res.end();return;}
  let body='';for await(const chunk of req)body+=chunk;
  receivedJob=JSON.parse(body);receivedResolve();
  await new Promise(resolve=>releaseAgent=resolve);
  const job=receivedJob;
  res.setHeader('Content-Type','application/json');
  res.end(JSON.stringify({id:randomUUID(),answer:'test frozen analysis',stages:[],createdAt:new Date().toISOString(),disclaimer:'test',transparency:{provider:job.config.provider,model:job.config.model,contextGroups:[],payloadBytes:job.context.payload_bytes,contextRevision:job.context.revision,memoryItemsUsed:0,externalDataUsed:false,apiKeySent:true}}));
}).listen(0,'127.0.0.1');
await once(mockAgent,'listening');
const agentUrl=`http://127.0.0.1:${mockAgent.address().port}`;

async function start(){
  const socket=createServer().listen(0,'127.0.0.1');await once(socket,'listening');const port=socket.address().port;await new Promise(r=>socket.close(r));
  const child=spawn(binary,['--port',String(port)],{env:{...process.env,DATABASE_URL:database,MARIO_MASTER_KEY:master,MARIO_REGISTRATION_KEY:invite,MARIO_HOST:'127.0.0.1',MARIO_WEB_DIR:resolve('client/dist'),MARIO_AGENT_URL:agentUrl,MARIO_AGENT_TOKEN:agentToken},stdio:['ignore','pipe','pipe']});
  children.push(child);child.stdout.on('data',b=>logs+=b);child.stderr.on('data',b=>logs+=b);
  const base=`http://127.0.0.1:${port}/api`;
  for(let i=0;i<150;i++){try {if((await fetch(base+'/server')).ok)return base;}catch{} if(child.exitCode!==null)throw Error(logs);await sleep(100);}
  throw Error('server not ready '+logs);
}
async function request(base,path,session,method='GET',body,expected=200,revision=session?.revision){
  const response=await fetch(base+path,{method,headers:{'Content-Type':'application/json',...(session?{Authorization:`Bearer ${session.token}`} : {}),...(revision?{'If-Match':revision}:{})},body:body===undefined?undefined:JSON.stringify(body)});
  const text=await response.text(); assert.equal(response.status,expected,`${method} ${path}: ${text}`);assertions++;
  if(session&&response.ok&&response.headers.get('x-data-revision')) session.revision=response.headers.get('x-data-revision');
  return text?JSON.parse(text):undefined;
}
async function register(base){return request(base,'/auth/register',null,'POST',{email:`${randomUUID()}@example.test`,password:'test-password-0123456789',registrationKey:invite});}
try{
  const base=await start(), replica=await start();
  await request(base,'/snapshot',null,'GET',undefined,401);
  await request(base,'/auth/register',null,'POST',{email:'blocked@example.test',password:'test-password-0123456789',registrationKey:'wrong'},401);
  const a=await register(base),b=await register(base);
  const first=await request(base,'/snapshot',a); assert.equal(first.holdings.length,0);
  assert.equal((await request(base,'/snapshot',b)).holdings.length,0);
  const emptyBackup=await request(base,'/data/export',b);
  await request(base,'/data/import',b,'POST',emptyBackup);
  const a2={...a};
  const profile={...first.profile,monthlyIncome:22000,monthlyExpense:8000};
  await request(base,'/profile',a,'PUT',profile);
  await request(replica,'/profile',a2,'PUT',{...profile,monthlyIncome:1},409);
  assert.equal((await request(base,'/snapshot',b)).profile.monthlyIncome,first.profile.monthlyIncome);
  assert.equal((await request(replica,'/snapshot',a2)).profile.monthlyIncome,22000);
  const holding={symbol:'CASH',name:'测试现金',assetClass:'现金',marketValue:45000,costBasis:0,targetPct:0,currency:'CNY',fxRateToBase:null,valuationDate:'2026-09-30',fxRateSource:'',fxRateObservedOn:''};
  const snap=await request(base,'/holdings',a,'POST',holding);const id=snap.holdings[0].id;
  await request(base,`/holdings/${id}`,b,'DELETE',undefined,400);
  await request(base,'/daily-assets/ensure',a,'POST',{timezone:'Asia/Shanghai'});
  const stable=await request(base,'/sync/version',a);
  await request(base,'/daily-assets/ensure',a,'POST',{timezone:'Asia/Shanghai'});
  assert.equal((await request(base,'/sync/version',a)).revision,stable.revision,'idle ensure must not create changes');
  await request(base,'/daily-assets?limit=20',a);
  const current=await request(base,'/snapshot',a);
  await request(base,`/holdings/${id}/amount`,a,'PUT',{amount:0,expectedRevision:current.holdingRevisions[id],requestId:randomUUID()});
  assert.equal((await request(base,'/snapshot',a)).holdings[0].marketValue,0);
  // Same version at two API replicas: precisely one write is allowed.
  const rev=a.revision;
  const race=await Promise.all([base,replica].map((endpoint,i)=>fetch(endpoint+'/profile',{method:'PUT',headers:{Authorization:`Bearer ${a.token}`,'Content-Type':'application/json','If-Match':rev},body:JSON.stringify({...profile,monthlyIncome:30000+i})})));
  assert.deepEqual(race.map(r=>r.status).sort(),[200,409]);assertions++;
  await request(base,'/snapshot',a);
  await request(base,'/model-config',a,'PUT',{provider:'openai-responses',baseUrl:'https://api.openai.com/v1',model:'test-model',apiKey:'test-secret-never-export'});
  assert.equal((await request(base,'/model-config',a)).hasApiKey,true);
  assert.equal((await request(base,'/model-config',b)).hasApiKey,false);
  await request(base,'/model-config/codex',a,'POST',{},400);
  await request(base,'/model-config',a,'PUT',{provider:'openai-responses',baseUrl:'http://127.0.0.1:1234',model:'test',apiKey:'secret'},400);
  const analysisRequest={question:'test authorized context',workflow:'quick',useMemory:false,reflect:false,exploreAlternatives:false,contextSelection:{includeProfile:false}};
  const preview=await request(base,'/analysis/preview',a,'POST',analysisRequest);
  const running=request(base,'/analysis',a,'POST',{...analysisRequest,previewRevision:preview.contextRevision});
  await Promise.race([received,new Promise((_,reject)=>{const t=setTimeout(()=>reject(Error('Agent never received job')),10000);t.unref();})]);
  assert.equal(receivedJob.api_key,'test-secret-never-export');
  assert.equal(receivedJob.context.payload.financialProfile,undefined);
  assert.deepEqual(Object.keys(receivedJob).sort(),['api_key','config','context','memories','request']);
  const concurrent={...a};await request(replica,'/snapshot',concurrent);
  await request(replica,'/profile',concurrent,'PUT',{...profile,monthlyIncome:77777});
  releaseAgent();await running;
  assert.equal((await request(base,'/snapshot',a)).profile.monthlyIncome,77777);
  assert.equal((await request(base,'/analyses',a)).length,1);
  const agentSocket=createServer().listen(0,'127.0.0.1');await once(agentSocket,'listening');const agentPort=agentSocket.address().port;await new Promise(r=>agentSocket.close(r));
  const worker=spawn(binary,[],{env:{...process.env,MARIO_ROLE:'agent',MARIO_AGENT_BIND:`127.0.0.1:${agentPort}`,MARIO_AGENT_TOKEN:agentToken},stdio:['ignore','pipe','pipe']});children.push(worker);
  worker.stdout.on('data',b=>logs+=b);worker.stderr.on('data',b=>logs+=b);
  const workerUrl=`http://127.0.0.1:${agentPort}`;
  for(let i=0;i<100;i++){try{if((await fetch(workerUrl+'/health',{headers:{Authorization:`Bearer ${agentToken}`}})).ok)break;}catch{} await sleep(50);}
  assert.equal((await fetch(workerUrl+'/health')).status,401);
  assert.equal((await fetch(workerUrl+'/api/snapshot',{headers:{Authorization:`Bearer ${agentToken}`}})).status,404);
  assert.equal((await fetch(workerUrl+'/execute',{method:'POST',headers:{Authorization:`Bearer ${agentToken}`,'Content-Type':'application/json'},body:JSON.stringify({...receivedJob,config:{...receivedJob.config,baseUrl:'http://127.0.0.1'}})})).status,400);
  console.log('PASS Agent isolation: authentication, no data API, endpoint allowlist, authorized context and non-blocking analysis');
  const backup=await request(base,'/data/export',a);
  assert(!JSON.stringify(backup).includes('test-secret'));
  const c=await register(base);await request(base,'/snapshot',c);await request(base,'/daily-assets/ensure',c,'POST',{timezone:'UTC'});
  const beforeFailedImport=c.revision;
  const invalid=structuredClone(backup);const holdingTable=invalid.tables.find(t=>t.name==='holdings');holdingTable.rows.push([...holdingTable.rows[0]]);
  await request(base,'/data/import',c,'POST',invalid,500);
  assert.equal((await request(base,'/snapshot',c)).holdings.length,0);assert.equal(c.revision,beforeFailedImport,'failed import rolls back the version');
  await request(base,'/data/import',c,'POST',backup);
  const restored=await request(base,'/snapshot',c);assert.equal(restored.holdings[0].id,id);
  await request(base,'/data/import',c,'POST',backup,409);
  // Deletion and version change are visible from another replica.
  await request(base,`/holdings/${id}`,a,'DELETE');
  assert.equal((await request(replica,'/snapshot',a2)).holdings.length,0);
  const auth=await request(base,'/auth/login',null,'POST',{email:a.email,password:'test-password-0123456789'});
  await request(base,'/auth/logout',auth,'POST',{});
  await request(base,'/snapshot',auth,'GET',undefined,401);
  const resetPassword='replacement-password-12345';
  const reset=spawn(binary,['--reset-password',b.email],{env:{...process.env,DATABASE_URL:database,MARIO_RESET_PASSWORD:resetPassword},stdio:['ignore','pipe','pipe']});
  reset.stdout.on('data',chunk=>logs+=chunk);reset.stderr.on('data',chunk=>logs+=chunk);
  assert.equal((await once(reset,'exit'))[0],0,'administrator password reset');
  await request(base,'/snapshot',b,'GET',undefined,401);
  const resetSession=await request(base,'/auth/login',null,'POST',{email:b.email,password:resetPassword});
  await request(base,'/snapshot',resetSession);
  const functional=await register(base);
  const suite=spawn(process.execPath,['scripts/test-functional-api.mjs'],{env:{...process.env,MARIO_FUNCTIONAL_BASE_URL:base,MARIO_FUNCTIONAL_TOKEN:functional.token},stdio:'inherit'});
  const [code]=await once(suite,'exit');assert.equal(code,0,'full domain suite on PostgreSQL');
  await request(base,'/cloud/status',a,'GET',undefined,404);
  assert.equal((await fetch(base.replace('/api','')+'/mario.db')).status,404);
  for (const child of children) { if(child.exitCode===null && child.signalCode===null){const ended=once(child,'exit');child.kill();await ended;} }
  // Restart keeps both data and unrevoked sessions.
  const restart=await start();assert.equal((await request(restart,'/model-config',a)).hasApiKey,true);
  assert.equal((await request(restart,'/snapshot',c)).holdings[0].id,id);
  console.log(`PASS hosted PostgreSQL: ${assertions} HTTP checks; tenant isolation, replica conflicts, durable revisions, secrets, migration, sessions and restart persistence`);
} catch(error){console.error(logs.slice(-5000));throw error;}
finally{releaseAgent?.();mockAgent.closeAllConnections();await new Promise(r=>mockAgent.close(r));for(const child of children){if(child.exitCode===null && child.signalCode===null){const ended=once(child,'exit');child.kill();await ended;}}}

