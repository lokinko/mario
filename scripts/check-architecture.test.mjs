import assert from 'node:assert/strict';
import test from 'node:test';
import {serverFileViolation,serverViolation,clientViolation,cycles} from './check-architecture.mjs';

test('public price storage cannot import account, application or source implementations',()=>{
  for(const target of ['accounts','db','application','ports','market_data','http','ai'])
    assert.ok(serverFileViolation('db/public_prices.rs',target),target);
  for(const target of ['models','storage','error'])
    assert.equal(serverFileViolation('db/public_prices.rs',target),undefined,target);
});
test('public price orchestration only uses public repositories, contracts and ports',()=>{
  for(const target of ['accounts','http','hosted','composition','market_data','ai'])
    assert.ok(serverFileViolation('application/public_prices.rs',target),target);
  for(const target of ['models','db','ports','error'])
    assert.equal(serverFileViolation('application/public_prices.rs',target),undefined,target);
});
test('price source injection preserves existing domain and storage dependency directions',()=>{
  for(const [source,target] of [['application','market_data'],['market_data','db'],['db','application'],['models','ports'],['models','market_data']])
    assert.ok(serverViolation(source,target),source+' -> '+target);
  assert.equal(serverViolation('application','ports'),undefined);
  assert.equal(serverViolation('market_data','models'),undefined);
});
test('shared price work does not relax UI isolation or cycle detection',()=>{
  assert.ok(clientViolation('features/portfolio/MyFacts.tsx','features/watch/WatchTrends.tsx'));
  assert.ok(clientViolation('lib/service.ts','api/portfolio.ts'));
  assert.equal(cycles(new Map([['a',new Set(['b'])],['b',new Set(['a'])]])).length,1);
});
