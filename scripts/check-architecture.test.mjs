import assert from 'node:assert/strict';
import { test } from 'node:test';
import { clientViolation, serverViolation, cycles, rustDependencies, checkArchitecture } from './check-architecture.mjs';

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
