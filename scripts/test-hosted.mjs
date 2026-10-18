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
const binary=resolve(process.env.MARIO_TEST_SERVER_BIN || 'server/target/debug/mario-server'+(process.platform==='win32'?'.exe':''));
const master=randomBytes(32).toString('base64');
const children=[]; let logs=''; let assertions=0;
const agentToken=randomBytes(32).toString('hex');
const modelRequests=[];
const mockModel=httpServer(async(req,res)=>{
  let body='';for await(const chunk of req)body+=chunk;
  modelRequests.push({path:req.url,auth:req.headers.authorization,body:JSON.parse(body)});
  res.setHeader('Content-Type','application/json');
  res.end(JSON.stringify({choices:[{message:{role:'assistant',content:'OK'},finish_reason:'stop'}],usage:{prompt_tokens:8,completion_tokens:1}}));
}).listen(0,'127.0.0.1');
await once(mockModel,'listening');
const modelUrl=`http://127.0.0.1:${mockModel.address().port}/custom/v1`;
let receivedJob, releaseAgent, receivedResolve;
const received=new Promise(resolve=>receivedResolve=resolve);
const mockAgent=httpServer(async(req,res)=>{
  if(req.headers.authorization!==`Bearer ${agentToken}`){res.writeHead(401);res.end();return;}
  let body='';for await(const chunk of req)body+=chunk;
  receivedJob=JSON.parse(body);receivedResolve();
  await new Promise(resolve=>releaseAgent=resolve);
  const job=receivedJob;
  res.setHeader('Content-Type','application/json');
  res.end(JSON.stringify({id:randomUUID(),answer:'test frozen analysis',stages:[],createdAt:new Date().toISOString(),disclaimer:'test',transparency:{provider:job.config.provider,model:job.config.model,contextGroups:[],payloadBytes:job.context.payload_bytes,contextRevision:job.context.revision,externalDataUsed:false,apiKeySent:true}}));
}).listen(0,'127.0.0.1');
await once(mockAgent,'listening');
const agentUrl=`http://127.0.0.1:${mockAgent.address().port}`;

async function start(){
  const socket=createServer().listen(0,'127.0.0.1');await once(socket,'listening');const port=socket.address().port;await new Promise(r=>socket.close(r));
  const child=spawn(binary,['--port',String(port)],{env:{...process.env,DATABASE_URL:database,MARIO_MASTER_KEY:master,MARIO_HOST:'127.0.0.1',MARIO_WEB_DIR:resolve('client/dist'),MARIO_AGENT_URL:agentUrl,MARIO_AGENT_TOKEN:agentToken},stdio:['ignore','pipe','pipe']});
  children.push(child);child.stdout.on('data',b=>logs+=b);child.stderr.on('data',b=>logs+=b);
  const base=`http://127.0.0.1:${port}/api`;
  for(let i=0;i<150;i++){try {if((await fetch(base+'/server')).ok)return base;}catch{} if(child.exitCode!==null)throw Error(logs);await sleep(100);}
  throw Error('server not ready '+logs);
}
async function request(base,path,session,method='GET',body,expected=200,revision=session?.revision){
  const response=await fetch(base+path,{method,headers:{'Content-Type':'application/json',...(session?{Authorization:`Bearer ${session.token}`} : {}),...(revision?{'If-Match':revision}:{})},body:body===undefined?undefined:JSON.stringify(body)});
  const text=await response.text(); assert.equal(response.status,expected,`${method} ${path}: ${text}`);assertions++;
  if(session&&response.ok&&response.headers.get('x-data-revision')) session.revision=response.headers.get('x-data-revision');
  return text?(response.headers.get("content-type")?.includes("application/json")?JSON.parse(text):text):undefined;
}
async function register(base){return request(base,'/auth/register',null,'POST',{email:`${randomUUID()}@example.test`,password:'test-password-0123456789'});}
try{
  const base=await start(), replica=await start();
  await request(base,'/snapshot',null,'GET',undefined,401);
  await request(base,'/auth/register',null,'POST',{email:`${randomUUID()}@example.test`,password:'test-password-0123456789',registrationKey:'obsolete'},422);
  await request(base,'/auth/register',null,'POST',{email:`${randomUUID()}@example.test`,password:'short'},400);
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
  await request(base,'/model-config',a,'PUT',{provider:'openai-responses',baseUrl:'https://gateway.example.test:8443/custom/v1',model:'test-model',apiKey:'test-secret-never-export'});
  assert.equal((await request(base,'/model-config',a)).hasApiKey,true);
  assert.equal((await request(base,'/model-config',b)).hasApiKey,false);
  await request(base,'/model-config/codex',a,'POST',{},404);
  await request(base,'/model-config',a,'PUT',{provider:'openai-responses',baseUrl:'https://gateway.example.test/v1?key=secret',model:'test',apiKey:'secret'},400);
  await request(base,'/model-config',b,'PUT',{provider:'openai-chat',baseUrl:modelUrl,model:'third-party-model',apiKey:'third-party-key'});
  assert.equal((await request(base,'/model-config',b)).provider,'openai-chat');
  assert.equal((await request(base,'/model-config/test',b,'POST')).ok,true);
  assert.equal(modelRequests[0].path,'/custom/v1/chat/completions');
  assert.equal(modelRequests[0].auth,'Bearer third-party-key');
  assert.equal(modelRequests[0].body.model,'third-party-model');
  assert.equal(modelRequests[0].body.messages[0].role,'system');
  assert.equal(modelRequests[0].body.stream,false);
  assert(!('input' in modelRequests[0].body));
  const analysisRequest={question:'test authorized context',workflow:'quick',reflect:false,exploreAlternatives:false,contextSelection:{includeProfile:false}};
  const preview=await request(base,'/analysis/preview',a,'POST',analysisRequest);
  const running=request(base,'/analysis',a,'POST',{...analysisRequest,previewRevision:preview.contextRevision});
  await Promise.race([received,new Promise((_,reject)=>{const t=setTimeout(()=>reject(Error('Agent never received job')),10000);t.unref();})]);
  assert.equal(receivedJob.api_key,'test-secret-never-export');
  assert.equal(receivedJob.config.baseUrl,'https://gateway.example.test:8443/custom/v1');
  assert.equal(receivedJob.context.payload.financialProfile,undefined);
  assert.deepEqual(Object.keys(receivedJob).sort(),['api_key','config','context','request']);
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
  assert.equal((await fetch(workerUrl+'/execute',{method:'POST',headers:{Authorization:`Bearer ${agentToken}`,'Content-Type':'application/json'},body:JSON.stringify({...receivedJob,config:{...receivedJob.config,baseUrl:'file:///etc/passwd'}})})).status,400);
  // The actual Agent must reach the same custom endpoint; this deliberately plain
  // response fails structured analysis validation after the model request succeeds.
  const beforeAgentCalls=modelRequests.length;
  await fetch(workerUrl+'/execute',{method:'POST',headers:{Authorization:`Bearer ${agentToken}`,'Content-Type':'application/json'},body:JSON.stringify({...receivedJob,api_key:'third-party-key',config:{provider:'openai-chat',baseUrl:modelUrl,model:'third-party-model',hasApiKey:true},request:{...receivedJob.request,webSearch:false}})});
  assert(modelRequests.length>beforeAgentCalls,'Agent must call the custom model URL');
  assert.equal(modelRequests.at(-1).path,'/custom/v1/chat/completions');
  assert.equal(modelRequests.at(-1).auth,'Bearer third-party-key');
  console.log('PASS custom model API and Agent isolation: custom hosts/ports/paths, Chat Completions, authentication, no data API, endpoint validation, authorized context and non-blocking analysis');
  const lostRequest={requestId:randomUUID(),newHolding:{name:'回执恢复测试',symbol:'',assetClass:'股票',currency:'CNY'},kind:'buy',occurredOn:'2026-01-01',sequence:1,quantity:'1',price:'10',grossAmount:'10',fees:'0',taxes:'0',account:'回执账户',settlementMode:'history'};
  const staleRevision=a.revision;
  const lostHeaders={Authorization:`Bearer ${a.token}`,'Content-Type':'application/json','If-Match':staleRevision};
  const lostResponse=await fetch(base+'/holding-trades',{method:'POST',headers:lostHeaders,body:JSON.stringify(lostRequest)});
  assert.equal(lostResponse.status,200);await lostResponse.json(); // Commit happened, but the caller lost its revision/result.
  const recoveredResponse=await fetch(replica+'/holding-trades',{method:'POST',headers:lostHeaders,body:JSON.stringify(lostRequest)});
  assert.equal(recoveredResponse.status,200,'stale revision must not hide a committed financial receipt');
  a.revision=recoveredResponse.headers.get('x-data-revision');const recovered=await recoveredResponse.json();
  assert.equal(recovered.holdingTrades.filter(t=>t.id===lostRequest.requestId).length,1);
  const otherReceipt=await request(base,'/financial-operations/resolve',b,'POST',{method:'POST',path:'/api/holding-trades',input:lostRequest});
  assert.equal(otherReceipt.committed,false,'receipts are isolated by authenticated account');
  const mismatched=await fetch(base+'/holding-trades',{method:'POST',headers:lostHeaders,body:JSON.stringify({...lostRequest,fees:'1'})});assert.equal(mismatched.status,409);
  const recoveredId=recovered.holdingTrades.find(t=>t.id===lostRequest.requestId).holdingId;
  await request(base,`/holding-trades/${lostRequest.requestId}`,a,'DELETE',{expectedRevision:recovered.holdingRevisions[recoveredId]});
  const trail=await request(base,`/holdings/${recoveredId}/financial-history`,a);assert.equal(trail.records.length,2);assert(trail.records.every(r=>r.operationId));
  await request(base,`/holdings/${recoveredId}`,a,'DELETE');
  const positionRequest={...lostRequest,requestId:randomUUID(),newHolding:{...lostRequest.newHolding,name:'整项删除回执测试'}};
  const position=await request(base,'/holding-trades',a,'POST',positionRequest);
  const positionId=position.holdingTrades.find(t=>t.id===positionRequest.requestId).holdingId;
  const removal={expectedRevision:position.holdingRevisions[positionId]};
  const removalHeaders={Authorization:`Bearer ${a.token}`,'Content-Type':'application/json','If-Match':a.revision};
  const removedPosition=await request(base,`/holdings/${positionId}`,a,'DELETE',removal);
  assert(!removedPosition.holdings.some(h=>h.id===positionId));
  const removalRetry=await fetch(replica+`/holdings/${positionId}`,{method:'DELETE',headers:removalHeaders,body:JSON.stringify(removal)});
  assert.equal(removalRetry.status,200,'stale global revision must still recover a whole-position deletion receipt');
  const recoveredRemoval=await removalRetry.json();
  assert(!recoveredRemoval.holdingTrades.some(t=>t.holdingId===positionId));
  a.revision=removalRetry.headers.get('x-data-revision');
  assert.equal((await request(base,'/financial-operations/resolve',b,'POST',{method:'DELETE',path:`/api/holdings/${positionId}`,input:removal})).committed,false);
  const verifiedCash=await request(base,'/snapshot',a);
  await request(base,`/holdings/${id}/amount`,a,'PUT',{amount:'0',expectedRevision:verifiedCash.holdingRevisions[id],requestId:randomUUID()});
  console.log('PASS financial HTTP receipts: lost response, stale retry across replicas, nonce mismatch, tenant isolation and permanent correction journal');
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
  // Leave non-empty exact accounting and company-action evidence for native restore verification.
  const financialAccount=await register(base);
  let financial=await request(base,'/snapshot',financialAccount);
  financial=await request(base,'/holdings',financialAccount,'POST',{...holding,valuationDate:new Date().toISOString().slice(0,10),marketValue:99999999999999.02,cashAmount:'99999999999999.02'});
  assert.equal(financial.cashBalances[0].amount,'99999999999999.02');
  const execution={...lostRequest,requestId:randomUUID(),newHolding:{...lostRequest.newHolding,name:'原生恢复公司行动'},account:'恢复账户'};
  financial=await request(base,'/holding-trades',financialAccount,'POST',execution);
  const security=financial.holdingTrades[0].holdingId;
  const action={requestId:randomUUID(),expectedRevision:financial.holdingRevisions[security],split:{effectiveOn:'2026-02-01',numerator:'2',denominator:'1'},reason:'券商通知核对测试'};
  financial=await request(base,`/holdings/${security}/corporate-actions`,financialAccount,'POST',action);
  assert.equal(financial.valuationStatus.decisionReady,false);
  financial=await request(base,`/corporate-actions/${action.requestId}/confirm`,financialAccount,'POST',{expectedRevision:financial.holdingRevisions[security],reason:'核对实际股数'});
  assert.equal(financial.holdingTradeSummaries[security].quantity,'2');
  assert.equal(financial.holdingTradeSummaries[security].costBasis,'10');
  assert.equal(financial.accounting.holdings[security].usable,false);
  financial=await request(base,`/holdings/${security}/amount`,financialAccount,'PUT',{amount:'12.34',requestId:randomUUID(),expectedRevision:financial.holdingRevisions[security]});
  assert.equal(financial.accounting.totalValue,'100000000000011.36');
  const financialBackup=await request(base,'/data/export',financialAccount);
  const financialCopy=await register(base);await request(base,'/snapshot',financialCopy);
  await request(base,'/data/import',financialCopy,'POST',financialBackup);
  const copied=await request(replica,'/snapshot',financialCopy);
  assert.equal(copied.accounting.totalValue,financial.accounting.totalValue);
  assert.deepEqual(copied.corporateActions,financial.corporateActions);
  assert.equal((await request(base,`/holdings/${security}/financial-history`,financialCopy)).records.length,4);
  console.log('PASS PostgreSQL exact-source and company-action lifecycle, JSON restore and replica reads');
  // Explicit account replacement must replace existing trades, preserve credentials,
  // remain isolated, roll back failed imports, and reject changes since the backup.
  await request(base,'/model-config',financialCopy,'PUT',{provider:'openai-chat',baseUrl:modelUrl,model:'target-model',apiKey:'target-key-preserved'});
  const targetTrade={...lostRequest,requestId:randomUUID(),newHolding:{...lostRequest.newHolding,name:'覆盖前独有交易'},account:'覆盖前账户'};
  const targetBefore=await request(base,'/holding-trades',financialCopy,'POST',targetTrade);
  const targetBackup=await request(base,'/data/export',financialCopy);
  await request(base,'/data/import',financialCopy,'POST',financialBackup,409);
  await request(base,'/data/import?replace=false',financialCopy,'POST',financialBackup,409);
  const replacementInvalid=structuredClone(financialBackup);
  const replacementHoldings=replacementInvalid.tables.find(t=>t.name==='holdings');
  replacementHoldings.rows.push([...replacementHoldings.rows[0]]);
  const beforeReplacementFailure=financialCopy.revision;
  await request(base,'/data/import?replace=true',financialCopy,'POST',replacementInvalid,500);
  assert.deepEqual((await request(base,'/data/export',financialCopy)).tables,targetBackup.tables,'failed replacement must preserve every original table');
  assert.equal(financialCopy.revision,beforeReplacementFailure,'failed replacement must roll back the revision');
  const staleReplacement={...financialCopy};
  await request(replica,'/profile',financialCopy,'PUT',{...targetBefore.profile,monthlyIncome:34567});
  await request(base,'/data/import?replace=true',staleReplacement,'POST',financialBackup,409);
  assert.equal((await request(base,'/snapshot',financialCopy)).profile.monthlyIncome,34567);
  await request(base,'/data/import?replace=true',financialCopy,'POST',financialBackup);
  const replaced=await request(replica,'/snapshot',financialCopy);
  assert.deepEqual(replaced.holdingTrades,financial.holdingTrades);
  assert.deepEqual(replaced.corporateActions,financial.corporateActions);
  assert.deepEqual(replaced.accounting,financial.accounting);
  assert.equal(replaced.profile.monthlyIncome,financial.profile.monthlyIncome);
  assert(!replaced.holdingTrades.some(t=>t.id===targetTrade.requestId));
  assert.equal((await request(base,`/holdings/${security}/financial-history`,financialCopy)).records.length,4);
  assert.deepEqual((await request(base,'/data/export',financialAccount)).tables,financialBackup.tables,'replacement must not modify the source account');
  const preservedModel=await request(base,'/model-config',financialCopy);
  assert.equal(preservedModel.model,'target-model');assert.equal(preservedModel.hasApiKey,true);
  await request(base,'/model-config/test',financialCopy,'POST');
  assert.equal(modelRequests.at(-1).auth,'Bearer target-key-preserved');
  await request(base,'/data/import?replace=true',financialCopy,'POST',targetBackup);
  const rolledBack=await request(replica,'/snapshot',financialCopy);
  assert.deepEqual(rolledBack.holdingTrades,targetBefore.holdingTrades,'the downloaded backup must restore original trades');
  assert.deepEqual(rolledBack.accounting,targetBefore.accounting);
  console.log('PASS account overwrite import: existing trades replaced, original backup restored, transaction rollback, revision conflicts, credential preservation and tenant isolation');
  await request(base,'/cloud/status',a,'GET',undefined,404);
  assert.equal((await fetch(base.replace('/api','')+'/mario.db')).status,404);
  for (const child of children) { if(child.exitCode===null && child.signalCode===null){const ended=once(child,'exit');child.kill();await ended;} }
  // Restart keeps both data and unrevoked sessions.
  const restart=await start();assert.equal((await request(restart,'/model-config',a)).hasApiKey,true);
  assert.equal((await request(restart,'/snapshot',c)).holdings[0].id,id);
  console.log(`PASS hosted PostgreSQL: ${assertions} HTTP checks; tenant isolation, replica conflicts, durable revisions, secrets, migration, sessions and restart persistence`);
} catch(error){console.error(logs.slice(-5000));throw error;}
finally{releaseAgent?.();mockAgent.closeAllConnections();await new Promise(r=>mockAgent.close(r));mockModel.closeAllConnections();await new Promise(r=>mockModel.close(r));for(const child of children){if(child.exitCode===null && child.signalCode===null){const ended=once(child,'exit');child.kill();await ended;}}}

