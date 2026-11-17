import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { rustRoutes, checkApiInventory, checkDocumentation } from './check-docs.mjs';

test('route inventory ignores comments, raw examples and test-only routers; keeps chained methods', () => {
  assert.deepEqual(rustRoutes(`
    // .route("/comment", get(comment))
    const EXAMPLE: &str = r#".route("/example", get(example))"#;
    fn router() { Router::new().route("/api/items", get(list).post(save))
      .route("/api/{*path}", axum::routing::any(fallback)) }
    #[cfg(test)] mod tests { fn fixture() { Router::new().route("/test", get(test)) } }
  `), ['GET /api/items', 'POST /api/items']);
  assert.throws(() => rustRoutes('fn router() { Router::new().route(PATH, get(list)) }'), /string literal/);
});
test('missing, stale and duplicate API entries fail, as do broken links and anchors', () => {
  const directory = mkdtempSync(join(tmpdir(), 'mario-docs-'));
  const put = (path, text) => writeFileSync(join(directory, path), text);
  try {
    for (const path of ['docs', 'server/src/http']) mkdirSync(join(directory, path), { recursive: true });
    put('README.md', '[索引](docs/api.md#接口)');
    put('AGENTS.md', '# Rules');
    put('server/src/hosted.rs', 'fn routes() { Router::new().route("/api/item", get(read).put(save)) }');
    put('server/src/agent.rs', '');
    const good = '# 接口\n| GET | `/api/item` | — |\n| PUT | `/api/item` | R |\n';
    const sourceFiles = [join(directory, 'server/src/hosted.rs')];
    assert.throws(() => checkApiInventory(sourceFiles, '| GET | `/api/item` | — |'), /inventory differs/);
    assert.throws(() => checkApiInventory(sourceFiles, good + '| GET | `/api/old` | — |'), /inventory differs/);
    assert.throws(() => checkApiInventory(sourceFiles, good + good), /duplicate documented/);
    put('docs/api.md', good);
    assert.equal(checkDocumentation(directory).routes, 2);
    put('README.md', '[broken](docs/missing.md)');
    assert.throws(() => checkDocumentation(directory), /missing.md/);
    put('README.md', '[anchor](docs/api.md#wrong)');
    assert.throws(() => checkDocumentation(directory), /missing anchor/);
    put('README.md', '[encoded](docs/api.md#%E6%8E%A5%E5%8F%A3)');
    assert.equal(checkDocumentation(directory).documents, 3);
    put('README.md', '<<<<<<< HEAD');
    assert.throws(() => checkDocumentation(directory), /conflict marker/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
