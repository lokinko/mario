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
  assert(serverSymbolViolation('application/model_settings.rs', 'ports::ai_account::AccountSession'));
  assert.equal(serverSymbolViolation('application/model_settings.rs', 'ports::account::AccountSession'), undefined);
});

test('user workflows retain only portfolio FX valuation or watch capabilities without AI state', async () => {
  const { serverSymbolViolation, rustPaths } = await import('./check-architecture.mjs');
  const { userAccountCapabilities } = await import('./architecture-boundaries.mjs');
  for (const [source, capabilities] of Object.entries(userAccountCapabilities)) {
    for (const target of ['db', 'storage', 'accounts']) assert(serverFileViolation(source, target));
    for (const target of ['ports::user_account', 'ports::user_account::self', 'ports::user_account::*',
      'ports::ai_account::ModelConfiguration', 'ports::model::ModelConnector',
      'application::analysis::AnalysisService', 'models::analysis::AnalysisSettings', 'models::connection::ModelConfig'])
      assert(serverSymbolViolation(source, target), `${source} -> ${target}`);
    for (const capability of capabilities)
      assert.equal(serverSymbolViolation(source, `ports::user_account::${capability}`), undefined);
  }
  assert(serverSymbolViolation('application/fx.rs', 'ports::user_account::PortfolioAccount'));
  assert(serverSymbolViolation('application/watch.rs', 'ports::user_account::ValuationAccount'));
  assert(serverSymbolViolation('application/analysis.rs', 'ports::user_account::PortfolioAccount'));
  assert(serverSymbolViolation('db/user_access.rs', 'models::provider::ModelProviderKind'));
  assert(serverSymbolViolation('db/user_access.rs', 'ports::ai_account'));
  assert(serverSymbolViolation('ports/account.rs', 'models::portfolio::Snapshot'));
  assert(serverSymbolViolation('db/account_access.rs', 'models::connection::ModelConfig'));
  assert.equal(serverSymbolViolation('db/account_access.rs', 'db::Database'), undefined);
  const imports = rustPaths('use super::super::{db as persistence, ports::user_account as all};', ['application', 'fx']);
  assert.deepEqual([...imports].sort(), ['db', 'ports::user_account']);
  for (const target of imports)
    assert(serverFileViolation('application/fx.rs', target) || serverSymbolViolation('application/fx.rs', target));
});

test('production AI inputs cannot regain the financial snapshot or ledger records', async () => {
  const { serverSymbolViolation } = await import('./check-architecture.mjs');
  for (const source of ['application/analysis.rs', 'ports/ai_account.rs', 'context.rs']) {
    for (const target of ['models::portfolio::Snapshot', 'models::portfolio::Snapshot::default', 'models::portfolio::HoldingTrade', 'models::portfolio', 'models::portfolio::self', 'models::portfolio::*', 'models::*', 'models'])
      assert(serverSymbolViolation(source, target));
    assert.equal(serverSymbolViolation(source, 'models::accounting::analysis::AuthorizedFinancialContext'), undefined);
  }
});

test('owned contracts preserve user to market and AI to authorized user directions', async () => {
  const { serverSymbolViolation, rustPaths } = await import('./check-architecture.mjs');
  const { clientBoundaryViolation } = await import('./architecture-boundaries.mjs');
  for (const [source, target] of [
    ['models/market/quotes.rs', 'models::portfolio::Holding'],
    ['models/market.rs', 'models::connection::ModelConfig'],
    ['models/fx.rs', 'models::analysis::ContextSelection'],
    ['models/accounting/analysis.rs', 'models::analysis::ContextSelection'],
    ['models/decimal.rs', 'models::portfolio::Holding'],
    ['application/portfolio.rs', 'models'],
    ['http/evidence.rs', 'models::*'],
    ['http/evidence.rs', 'models::research::*'],
    ['http/evidence.rs', 'models::self'],
  ]) assert(serverSymbolViolation(source, target), `${source} -> ${target}`);
  for (const [source, target] of [
    ['models/fx.rs', 'models::market::quotes::FxRateQuote'],
    ['models/analysis.rs', 'models::portfolio::HoldingValuationEvidence'],
    ['models/portfolio.rs', 'models::decimal'],
    ['models/market/history.rs', 'models::market::quotes::SecurityPriceQuote'],
  ]) assert.equal(serverSymbolViolation(source, target), undefined, `${source} -> ${target}`);
  const nested = rustPaths('use super::super::analysis::ContextSelection as Permission;', ['models', 'accounting', 'analysis']);
  assert.deepEqual([...nested], ['models::analysis::ContextSelection']);
  for (const target of nested) assert(serverSymbolViolation('models/accounting/analysis.rs', target));
  for (const [source, target] of [
    ['types/user.ts', 'types/ai.ts'], ['types/market.ts', 'types/user.ts'],
    ['types/market.ts', 'types/ai.ts'], ['types/user.ts', 'api/portfolio.ts'],
    ['features/portfolio/MyFacts.tsx', 'types/ai.ts'], ['api/portfolio.ts', 'types/ai.ts'],
  ]) assert(clientViolation(source, target), `${source} -> ${target}`);
  for (const [source, target] of [['types/user.ts', 'types/market.ts'], ['types/ai.ts', 'types/user.ts']])
    assert.equal(clientViolation(source, target), undefined);
  assert(clientBoundaryViolation('types/unowned.ts'));
  assert.equal(clientBoundaryViolation('types/market.ts'), undefined);
});

test('market sources and public repositories never depend on user watch contracts or account ports', async () => {
  const { serverSymbolViolation, rustPaths } = await import('./check-architecture.mjs');
  for (const source of ['market_data/watch.rs', 'market_data/watch_catalog.rs', 'market_data/watch_archive.rs',
    'ports/market.rs', 'application/watch_market.rs', 'application/public_prices/monitor.rs', 'db/public_dividends.rs']) {
    for (const target of ['models::watch', 'models::watch::WatchInput', 'models::watch::MIN_YIELD_PCT',
      'models::portfolio::Snapshot', 'models::analysis::ContextSelection', 'ports::user_account',
      'ports::account::AccountSession', 'application::watch::refresh'])
      assert(serverSymbolViolation(source, target), `${source} -> ${target}`);
    for (const target of ['models::market::dividends::DividendHistory', 'models::market::series::SecurityMarketData',
      'models::market::securities::SecuritySources'])
      assert.equal(serverSymbolViolation(source, target), undefined, `${source} -> ${target}`);
    for (const target of ['db', 'db::Database', 'db::PublicWatch'])
      assert(serverSymbolViolation(source, target), `${source} -> ${target}`);
  }
  assert.equal(serverSymbolViolation('application/watch_export.rs', 'models::watch::MIN_YIELD_PCT'), undefined);
  assert.equal(serverSymbolViolation('application/watch_export.rs', 'models::market::dividend_archive::DividendArchive'), undefined);
  assert(serverViolation('application', 'market_data'));
  assert(serverSymbolViolation('application/analysis.rs', 'application::watch'));
  assert(serverSymbolViolation('application/watch_catalog.rs', 'application::model_settings'));
  assert.equal(serverSymbolViolation('application/watch_catalog.rs', 'ports::market::SecuritySourceProvider'), undefined);
  const paths = rustPaths('use super::super::{models::watch as personal, ports::user_account as account};', ['application', 'watch_market']);
  assert.deepEqual([...paths].sort(), ['models::watch', 'ports::user_account']);
  for (const path of paths) assert(serverSymbolViolation('application/watch_market.rs', path));
});

test('business HTTP adapters only receive their sealed scope and owned record capabilities', async () => {
  const { serverSymbolViolation, rustPaths } = await import('./check-architecture.mjs');
  const { httpCapabilities, httpRegistryViolations, serverHttpOwners, httpOwnedRoutes } = await import('./architecture-boundaries.mjs');
  assert.deepEqual(httpRegistryViolations(serverHttpOwners, httpCapabilities, httpOwnedRoutes), []);
  assert(httpRegistryViolations({ ...serverHttpOwners, unscoped: 'user' }, httpCapabilities, httpOwnedRoutes).some(issue => issue.includes('unscoped')));
  const unregistered = { ...httpCapabilities, 'http/new.rs': { context: 'EvidenceContext', ports: [] } };
  assert(httpRegistryViolations(serverHttpOwners, unregistered, httpOwnedRoutes).some(issue => issue.includes('one adapter')));
  assert(httpRegistryViolations(serverHttpOwners, unregistered, httpOwnedRoutes).some(issue => issue.includes('stale')));
  for (const [source, scope] of Object.entries(httpCapabilities)) {
    assert.equal(serverSymbolViolation(source, `http::context::${scope.context}`), undefined);
    for (const target of ['http', 'http::RequestContext', 'http::RequestContextParts', 'http::context',
      'http::context::self', 'http::context::BackupContext', 'http::context::*'])
      assert(serverSymbolViolation(source, target), `${source} -> ${target}`);
    for (const other of Object.values(httpCapabilities).filter(other => other.context !== scope.context))
      assert(serverSymbolViolation(source, `http::context::${other.context}`));
    for (const target of ['db', 'storage', 'accounts']) assert(serverFileViolation(source, target));
    for (const port of scope.ports)
      assert.equal(serverSymbolViolation(source, `ports::${port}`), undefined, `${source} -> ${port}`);
    for (const target of ['ports', 'ports::user_records', 'ports::ai_records', 'ports::user_account', 'ports::ai_account'])
      assert(serverSymbolViolation(source, target));
  }
  const paths = rustPaths('use super::{context as all, RequestContext}; use super::context::ModelSettingsContext as Model;', ['http', 'evidence']);
  assert.deepEqual([...paths].sort(), ['http::RequestContext', 'http::context', 'http::context::ModelSettingsContext']);
  for (const target of paths) assert(serverSymbolViolation('http/evidence.rs', target));
  assert(serverSymbolViolation('application/fx.rs', 'ports::user_records::LedgerAccount'));
  assert(serverSymbolViolation('application/analysis.rs', 'ports::ai_records::AnalysisArchive'));
});
