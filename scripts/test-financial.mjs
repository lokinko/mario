// Full financial acceptance runs only against a cluster created by this process.
import assert from 'node:assert/strict';
import { resolve, join } from 'node:path';
import { root, run, withDisposableCluster } from './postgres-cluster.mjs';

const suffix = process.platform === 'win32' ? '.exe' : '';
await withDisposableCluster('mario-financial-', async ({ pg, pgEnv, createDatabase, directory }) => {
  const target = resolve(root, '.runtime/financial-build');
  const env = { MARIO_TEST_DATABASE_URL: await createDatabase('mario_financial'), MARIO_TEST_SERVER_BIN: join(target, 'debug', 'mario-server' + suffix) };
  await run('cargo', ['test', '--manifest-path', 'server/Cargo.toml', 'db::postgres_tests', '--', '--test-threads=2'], env);
  await run('cargo', ['build', '--manifest-path', 'server/Cargo.toml', '--target-dir', target], env);
  await run(process.execPath, ['scripts/test-functional-api.mjs'], env);
  await run(process.execPath, ['scripts/test-hosted.mjs'], env);
  // Recover the full database, including the financial journal, operation receipts,
  // exact balances, company actions, authentication data and encrypted secrets.
  const dump = join(directory, 'database.dump');
  await run(pg('pg_dump'), ['-Fc', '--file', dump, 'mario_financial'], pgEnv, true);
  await createDatabase('mario_restored');
  await run(pg('pg_restore'), ['--exit-on-error', '--single-transaction', '--dbname', 'mario_restored', dump], pgEnv, true);
  const tables = (await run(pg('psql'), ['-X', '-At', '-c', "SELECT schemaname||'.'||tablename FROM pg_tables WHERE schemaname='mario_auth' OR schemaname LIKE 'user_%' ORDER BY 1"], { ...pgEnv, PGDATABASE: 'mario_financial' }, true)).trim().split(/\r?\n/).filter(Boolean);
  assert(tables.length > 0, 'acceptance must create real account data');
  for (const table of tables) {
    assert(/^(?:mario_auth|user_[a-f0-9]+)\.[a-z_]+$/.test(table));
    const sql = `SELECT md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text)::text,'[]')) FROM ${table} t`;
    const source = await run(pg('psql'), ['-X', '-At', '-c', sql], { ...pgEnv, PGDATABASE: 'mario_financial' }, true);
    const restored = await run(pg('psql'), ['-X', '-At', '-c', sql], { ...pgEnv, PGDATABASE: 'mario_restored' }, true);
    assert.equal(restored, source, `restored rows differ: ${table}`);
  }
  console.log(`PASS financial release gate: PostgreSQL migrations, hosted HTTP workflows, replica isolation, exact accounting and native backup/restore (${tables.length} tables)`);
});
