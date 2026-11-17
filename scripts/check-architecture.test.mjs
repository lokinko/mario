import assert from 'node:assert/strict';
import { test } from 'node:test';
import { clientViolation, serverViolation, serverFileViolation, cycles, rustDependencies, rustModules, checkArchitecture } from './check-architecture.mjs';

test('rejects reverse dependencies and cross-feature UI while allowing ports and domain data', () => {
  assert(clientViolation('features/advisor/Report.tsx', 'features/reviews/List.tsx'));
  assert(clientViolation('lib/transport.ts', 'api/analysis.ts'));
  assert.equal(clientViolation('features/advisor/Report.tsx', 'components/PageHeader.tsx'), undefined);
  for (const [from, to] of [['models','ai'], ['ai','db'], ['db','application'], ['db','market_data'], ['application','agent'], ['agent','db']]) assert(serverViolation(from, to));
  assert(serverViolation('application', 'ai'));
  assert.equal(serverViolation('db', 'models'), undefined);
});
test('detects a real cycle and permits shared acyclic dependencies', () => {
  assert.equal(cycles(new Map([['a', new Set(['b'])], ['b', new Set(['a'])]])).length, 1);
  assert.deepEqual(cycles(new Map([['a', new Set(['c'])], ['b', new Set(['c'])]])), []);
});
test('Rust scan handles nested imports, prompt strings, characters, comments, and test-only items', () => {
  const source = `use crate::{db::Database, models::{Holding, Snapshot}, AppResult};
    // crate::http::Handler
    const PROMPT: &str = r#"crate::agent::run { ignore }"#;
    fn brace() { let value = '{'; crate::risk::analyze(); }
    #[cfg(test)] mod tests { use crate::hosted; fn f() { let v = "}"; } }
    #[cfg(test)] fn example() { crate::storage::Connection; }
    use crate::valuation;`;
  assert.deepEqual([...rustDependencies(source)].sort(), ['AppResult', 'db', 'models', 'risk', 'valuation']);
});
test('the checked-out project respects its boundaries', () => { checkArchitecture(); });
test('production module registration excludes test-only declarations with attributes', () => {
  assert.deepEqual([...rustModules(`mod accounts;
    #[cfg(test)]
    #[path = "contract_tests.rs"]
    mod contract_tests;
    pub(crate) mod application;
    // mod ghost;
    const EXAMPLE: &str = "mod ghost;";
    #[cfg(test)] mod tests { mod nested; }
  `)], ['accounts', 'application']);
});

test('site activity cannot inspect tenant ledgers or private workflows', () => {
  for (const target of ['db', 'application', 'ai', 'http', 'market_data', 'context'])
    assert(serverFileViolation('accounts/activity.rs', target));
  for (const target of ['accounts', 'storage', 'error'])
    assert.equal(serverFileViolation('accounts/activity.rs', target), undefined);
});
test('shared public dividend data stays separate from accounts and private analysis', () => {
  for (const target of ['accounts', 'db', 'application', 'market_data'])
    assert(serverFileViolation('db/public_dividends.rs', target));
  for (const target of ['accounts', 'context', 'ai', 'market_data'])
    assert(serverFileViolation('application/watch_market.rs', target));
  assert.equal(serverFileViolation('db/public_dividends.rs', 'storage'), undefined);
  assert.equal(serverFileViolation('application/watch_market.rs', 'ports'), undefined);
  for (const target of ['accounts', 'context', 'ai', 'market_data', 'http']) {
    assert(serverFileViolation('db/public_watch.rs', target));
    assert(serverFileViolation('application/watch_catalog.rs', target));
  }
  assert(serverFileViolation('db/public_watch.rs', 'db'));
  assert.equal(serverFileViolation('db/public_watch.rs', 'storage'), undefined);
  assert.equal(serverFileViolation('application/watch_catalog.rs', 'ports'), undefined);
  for (const target of ['accounts', 'context', 'ai', 'http', 'application'])
    assert(serverFileViolation('application/dividend_calendar.rs', target));
  assert(serverFileViolation('db/public_dividend_plans.rs', 'accounts'));
  assert.equal(serverFileViolation('application/dividend_calendar.rs', 'ports'), undefined);
  for (const target of ['accounts', 'db', 'storage', 'application', 'ai', 'http'])
    assert(serverFileViolation('market_data/watch_catalog.rs', target));
  assert.equal(serverFileViolation('market_data/watch_catalog.rs', 'models'), undefined);
  assert.equal(serverFileViolation('market_data/watch_catalog.rs', 'ports'), undefined);
});
test('local ChatGPT credentials cannot leak into hosted workflows or domain state', () => {
  assert.equal(serverViolation('hosted', 'openai_login'), undefined);
  for (const source of ['ai', 'agent', 'accounts', 'db', 'models']) assert(serverViolation(source, 'openai_login'));
  for (const target of ['chatgpt_auth', 'accounts', 'storage', 'ai']) assert(serverViolation('openai_login', target));
  for (const source of ['db','models','http','hosted','agent','application','ports']) assert(serverViolation(source, 'chatgpt_auth'));
  for (const target of ['db','accounts','ai','application','models']) assert(serverViolation('chatgpt_auth', target));
  assert.equal(serverViolation('runtime', 'chatgpt_auth'), undefined);
  assert.equal(serverViolation('codex_bridge_worker', 'chatgpt_auth'), undefined);
  assert.equal(serverFileViolation('ai/chatgpt.rs', 'chatgpt_auth'), undefined);
  assert(serverFileViolation('ai/orchestrator.rs', 'chatgpt_auth'));
});

test('domain clients can split internally without importing another domain', () => {
  assert.equal(clientViolation('api/portfolio.ts', 'api/portfolio/financialRequest.ts'), undefined);
  assert(clientViolation('api/analysis.ts', 'api/portfolio/financialRequest.ts'));
  assert(clientViolation('api/portfolio/financialRequest.ts', 'api/settings.ts'));
  assert(clientViolation('features/reviews/ReviewCenter.tsx', 'app/navigation.ts'));
});
test('market ports and workflows cannot reach concrete providers or account-writing adapters', () => {
  for (const [source, target] of [['secrets', 'db'], ['ports', 'ai'], ['ports', 'db'], ['models', 'db'], ['models', 'application'], ['market_data', 'accounts'], ['application', 'market_data']])
    assert(serverViolation(source, target), `${source} -> ${target}`);
  assert.equal(serverViolation('application', 'ports'), undefined);
  assert.equal(serverViolation('prices', 'ports'), undefined);
  assert(serverFileViolation('application/analysis.rs', 'ai'));
});
test('relative and grouped Rust paths cannot bypass dependency checks', () => {
  assert.deepEqual([...rustDependencies('use super::{db::Database, ai::Provider};', ['prices'])].sort(), ['ai', 'db']);
  assert.deepEqual([...rustDependencies('use super::super::db::Database; self::helper(); super::fx::refresh();', ['application', 'prices'])].sort(), ['application', 'db']);
  assert.deepEqual([...rustDependencies('use crate::{models::Snapshot, ports::{market::FxRateProvider}};', ['application', 'prices'])].sort(), ['models', 'ports']);
});


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

test('public collection monitoring preserves public orchestration boundaries', () => {
  for (const source of ['application/public_prices/monitor.rs', 'db/public_prices/monitor.rs'])
    for (const target of ['accounts', 'http', 'ai', 'market_data']) assert(serverFileViolation(source, target));
  assert.equal(serverFileViolation('application/public_prices/monitor.rs', 'ports'), undefined);
  assert.equal(serverFileViolation('db/public_prices/monitor.rs', 'storage'), undefined);
});

test('site model catalogs only depend on execution ports and errors', () => {
  for (const target of ['ports', 'error']) assert.equal(serverFileViolation('ai/model_catalog.rs', target), undefined);
  for (const target of ['db', 'accounts', 'context', 'application', 'http', 'ai', 'models']) assert(serverFileViolation('ai/model_catalog.rs', target));
});

test('new modules require an explicit boundary instead of silently escaping the rules', async () => {
  const { serverLayers, clientLayers, boundaryOf, clientBoundaryViolation } = await import('./architecture-boundaries.mjs');
  for (const layers of [serverLayers, clientLayers]) {
    const names = Object.values(layers).flat();
    assert.equal(new Set(names).size, names.length, 'each module has exactly one technical owner');
  }
  assert.equal(boundaryOf(serverLayers, 'new_workflow'), undefined);
  assert.equal(boundaryOf(serverLayers, 'application'), 'application');
  assert(clientBoundaryViolation('features/new-domain/Page.tsx'));
  assert(clientBoundaryViolation('api/new-domain.ts'));
  assert(clientBoundaryViolation('new-shared/file.ts'));
  assert.equal(clientBoundaryViolation('features/watch/BuyWatch.tsx'), undefined);
  assert.equal(clientBoundaryViolation('api/portfolio/financialRequest.ts'), undefined);
});

test('user features stay independent of AI while analysis can read user evidence through its API', async () => {
  const { featureModules, apiModules, clientFeatureOwners, clientApiOwners } = await import('./architecture-boundaries.mjs');
  assert.deepEqual(Object.keys(clientFeatureOwners).sort(), [...featureModules].sort());
  assert.deepEqual(Object.keys(clientApiOwners).sort(), [...apiModules].sort());
  for (const feature of ['evidence', 'methodology', 'portfolio', 'watch']) {
    for (const api of ['analysis', 'settings', 'memory'])
      assert(clientViolation(`features/${feature}/Page.tsx`, `api/${api}.ts`));
  }
  assert.equal(clientViolation('features/evidence/Page.tsx', 'api/evidence.ts'), undefined);
  assert.equal(clientViolation('features/methodology/Page.tsx', 'api/methodology.ts'), undefined);
  assert.equal(clientViolation('features/advisor/ReportSources.tsx', 'api/evidence.ts'), undefined);
});

test('user evidence and methodology routes cannot be mixed back into AI adapters', async () => {
  const { httpRouteViolation } = await import('./architecture-boundaries.mjs');
  assert(httpRouteViolation('http/analysis.rs', 'POST /api/research-evidence'));
  assert(httpRouteViolation('http/model_settings.rs', 'PUT /api/investment-methodology'));
  assert(httpRouteViolation('http/evidence.rs', 'GET /api/analyses'));
  assert(httpRouteViolation('http/other.rs', 'GET /api/analysis/preview'));
  assert.equal(httpRouteViolation('http/evidence.rs', 'PUT /api/research-evidence/{id}/tags'), undefined);
  assert.equal(httpRouteViolation('http/analysis.rs', 'GET /api/analyses/{id}'), undefined);
});

test('public dividend plan capabilities exclude user catalogs and user projection cannot fetch', async () => {
  const { rustPaths, serverSymbolViolation } = await import('./check-architecture.mjs');
  const source = `use crate::{db::{PublicWatch as Store, PublicDividendPlans}, models::{watch::WatchInput, market::dividend_plans::PlanSnapshot}};
    use super::super::db as persistence;
    fn f() { crate::db::Database::test(); }
    #[cfg(test)] mod tests { use crate::accounts; }`;
  const paths = [...rustPaths(source, ['application', 'dividend_plans'])];
  for (const expected of ['db::PublicWatch', 'db::PublicDividendPlans', 'models::watch::WatchInput', 'models::market::dividend_plans::PlanSnapshot', 'db', 'db::Database::test']) assert(paths.includes(expected), expected);
  assert(!paths.includes('accounts'));
  for (const path of ['db::PublicWatch', 'db', 'models::watch::WatchInput', 'application::dividend_calendar'])
    assert(serverSymbolViolation('application/dividend_plans.rs', path));
  for (const path of ['db::PublicDividendPlans', 'models::market::dividend_plans::PlanSnapshot', 'ports::market::DividendPlanProvider'])
    assert.equal(serverSymbolViolation('application/dividend_plans.rs', path), undefined);
  for (const path of ['ports::market::DividendPlanProvider', 'application::dividend_plans::PublicDividendPlanService'])
    assert(serverSymbolViolation('application/dividend_calendar.rs', path));
  assert.equal(serverSymbolViolation('application/dividend_calendar.rs', 'ports::market::DividendPlanReader'), undefined);
});

test('AI use cases cannot regain concrete account state or widen their declared capability', async () => {
  const { serverSymbolViolation } = await import('./check-architecture.mjs');
  for (const file of ['analysis', 'memory', 'model_settings', 'codex_bridge']) {
    for (const target of ['db', 'storage', 'accounts'])
      assert(serverFileViolation(`application/${file}.rs`, target));
    assert(serverSymbolViolation(`application/${file}.rs`, 'ports::ai_account'));
  }
  assert(serverSymbolViolation('application/analysis.rs', 'ports::ai_account::ModelSettingsStore'));
  assert(serverSymbolViolation('application/memory.rs', 'ports::ai_account::UserAnalysisSources'));
  assert.equal(serverSymbolViolation('application/analysis.rs', 'ports::ai_account::AnalysisAccount'), undefined);
  assert.equal(serverSymbolViolation('application/memory.rs', 'ports::ai_account::MemoryAccount'), undefined);
  assert.equal(serverFileViolation('application/analysis.rs', 'ports'), undefined);
});

test('production AI inputs cannot regain the financial snapshot or ledger records', async () => {
  const { serverSymbolViolation } = await import('./check-architecture.mjs');
  for (const source of ['application/analysis.rs', 'ports/ai_account.rs', 'context.rs']) {
    for (const target of ['models::Snapshot', 'models::HoldingTrade', 'models::*', 'models'])
      assert(serverSymbolViolation(source, target));
    assert.equal(serverSymbolViolation(source, 'models::accounting::analysis::AuthorizedFinancialContext'), undefined);
  }
});
