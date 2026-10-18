import assert from 'node:assert/strict';
import { test } from 'node:test';
import { clientViolation, serverViolation, serverFileViolation, cycles, rustDependencies, checkArchitecture } from './check-architecture.mjs';

test('rejects reverse dependencies and cross-feature UI while allowing ports and domain data', () => {
  assert(clientViolation('features/research/Report.tsx', 'features/memory/List.tsx'));
  assert(clientViolation('lib/transport.ts', 'api/research.ts'));
  assert.equal(clientViolation('features/research/Report.tsx', 'components/PageHeader.tsx'), undefined);
  for (const [from, to] of [['models','ai'], ['ai','db'], ['db','prices'], ['db','market_data'], ['application','agent'], ['agent','db']]) assert(serverViolation(from, to));
  assert.equal(serverViolation('application', 'ai'), undefined);
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

test('domain clients can split internally without importing another domain', () => {
  assert.equal(clientViolation('api/portfolio.ts', 'api/portfolio/financialRequest.ts'), undefined);
  assert(clientViolation('api/research.ts', 'api/portfolio/financialRequest.ts'));
  assert(clientViolation('api/portfolio/financialRequest.ts', 'api/settings.ts'));
  assert(clientViolation('features/reviews/ReviewCenter.tsx', 'app/navigation.ts'));
});
test('market ports and workflows cannot reach concrete providers or account-writing adapters', () => {
  for (const [source, target] of [['secrets', 'db'], ['ports', 'ai'], ['ports', 'db'], ['prices', 'db'], ['prices', 'application'], ['market_data', 'accounts'], ['application', 'market_data'], ['application', 'prices']])
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
