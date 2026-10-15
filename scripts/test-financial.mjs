// Full financial acceptance runs only against a cluster created by this process.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { resolve, join, dirname, basename } from 'node:path';
import { tmpdir } from 'node:os';
import { randomBytes } from 'node:crypto';
import { createServer } from 'node:net';

const root=resolve(import.meta.dirname,'..');
const pgBin=process.env.MARIO_TEST_PG_BIN;
assert(pgBin,'Set MARIO_TEST_PG_BIN to PostgreSQL bin. This gate creates its own disposable cluster and never reads .env or DATABASE_URL.');
const suffix=process.platform==='win32'?'.exe':'';
const pg=name=>join(resolve(pgBin),name+suffix);
const work=await mkdtemp(join(tmpdir(),'mario-financial-'));
const data=join(work,'data');
const password=randomBytes(24).toString('hex');
const passwordFile=join(work,'password');
let started=false;
async function run(command,args,env={},capture=false) {
  return await new Promise((accept,reject)=>{
    const childEnv={...process.env,...env};
    delete childEnv.PGSERVICE;delete childEnv.PGSERVICEFILE;delete childEnv.PGOPTIONS;
    const child=spawn(command,args,{cwd:root,env:childEnv,windowsHide:true,stdio:capture?['ignore','pipe','pipe']:'inherit'});
    let output='',error='';
    child.stdout?.on('data',chunk=>output+=chunk);
    child.stderr?.on('data',chunk=>error+=chunk);
    child.once('error',reject);
    child.once('exit',code=>code===0?accept(output):reject(Error(`${basename(command)} failed (${code})\n${error.slice(-4000)}`)));
  });
}
const socket=createServer();await new Promise(r=>socket.listen(0,'127.0.0.1',r));const port=socket.address().port;await new Promise(r=>socket.close(r));
const pgEnv={PGHOST:'127.0.0.1',PGPORT:String(port),PGUSER:'mario_test',PGPASSWORD:password,PGDATABASE:'postgres',PGSERVICE:'',PGSERVICEFILE:'',PGOPTIONS:''};
const connection=name=>`postgresql://mario_test:${password}@127.0.0.1:${port}/${name}`;
try {
  await writeFile(passwordFile,password,{mode:0o600});
  await run(pg('initdb'),['-D',data,'-U','mario_test','-A','scram-sha-256','--pwfile',passwordFile,'--encoding=UTF8','--no-locale'],{},true);
  await run(pg('pg_ctl'),['-D',data,'-l',join(work,'postgres.log'),'-o',`-h 127.0.0.1 -p ${port}`,'-w','start'],{},true);
  started=true;
  await run(pg('psql'),['-X','--set=ON_ERROR_STOP=1','-c','CREATE DATABASE mario_financial'],pgEnv,true);
  const target=resolve(root,'.runtime/financial-build');
  const env={MARIO_TEST_DATABASE_URL:connection('mario_financial'),MARIO_TEST_SERVER_BIN:join(target,'debug','mario-server'+suffix)};
  await run('cargo',['test','--manifest-path','server/Cargo.toml','db::postgres_tests','--','--ignored','--test-threads=2'],env);
  await run('cargo',['build','--manifest-path','server/Cargo.toml','--target-dir',target],env);
  await run(process.execPath,['scripts/test-functional-api.mjs'],env);
  await run(process.execPath,['scripts/test-hosted.mjs'],env);
  // Recover the full database, including the financial journal, operation receipts,
  // exact balances, company actions, authentication data and encrypted secrets.
  const dump=join(work,'database.dump');
  await run(pg('pg_dump'),['-Fc','--file',dump,'mario_financial'],pgEnv,true);
  await run(pg('psql'),['-X','--set=ON_ERROR_STOP=1','-c','CREATE DATABASE mario_restored'],pgEnv,true);
  await run(pg('pg_restore'),['--exit-on-error','--single-transaction','--dbname','mario_restored',dump],pgEnv,true);
  const tables=(await run(pg('psql'),['-X','-At','-c',"SELECT schemaname||'.'||tablename FROM pg_tables WHERE schemaname='mario_auth' OR schemaname LIKE 'user_%' ORDER BY 1"],{...pgEnv,PGDATABASE:'mario_financial'},true)).trim().split(/\r?\n/).filter(Boolean);
  assert(tables.length>0,'acceptance must create real account data');
  for(const table of tables) {
    assert(/^(?:mario_auth|user_[a-f0-9]+)\.[a-z_]+$/.test(table));
    const sql=`SELECT md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text)::text,'[]')) FROM ${table} t`;
    const source=await run(pg('psql'),['-X','-At','-c',sql],{...pgEnv,PGDATABASE:'mario_financial'},true);
    const restored=await run(pg('psql'),['-X','-At','-c',sql],{...pgEnv,PGDATABASE:'mario_restored'},true);
    assert.equal(restored,source,`restored rows differ: ${table}`);
  }
  console.log(`PASS financial release gate: PostgreSQL migrations, SQLite/hosted HTTP workflows, replica isolation, exact accounting and native backup/restore (${tables.length} tables)`);
} finally {
  if(started)await run(pg('pg_ctl'),['-D',data,'-m','fast','-w','stop'],{},true);
  assert.equal(dirname(resolve(work)),resolve(tmpdir()));assert(basename(work).startsWith('mario-financial-'));
  await rm(work,{recursive:true,force:true,maxRetries:20,retryDelay:100});
}
