import assert from 'node:assert/strict';
import { test } from 'node:test';
import { run } from './postgres-cluster.mjs';

const keys = ['MARIO_FUNCTIONAL_BASE_URL', 'MARIO_FUNCTIONAL_TOKEN', 'MARIO_TEST_DATABASE_URL', 'PGSERVICE', 'PGSERVICEFILE', 'PGOPTIONS'];
const probe = `process.stdout.write(JSON.stringify(Object.fromEntries(${JSON.stringify(keys)}.map(key => [key, process.env[key] ?? null]))))`;

async function withParentEnvironment(values, work) {
  const previous = Object.fromEntries(Object.keys(values).map(key => [key, process.env[key]]));
  Object.assign(process.env, values);
  try {
    return await work();
  } finally {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

async function childEnvironment(env = {}) {
  return JSON.parse(await run(process.execPath, ['-e', probe], env, true));
}

test('disposable test commands discard inherited functional targets without changing the parent', async () => {
  const parent = { MARIO_FUNCTIONAL_BASE_URL: 'https://parent-target.example.invalid/api', MARIO_FUNCTIONAL_TOKEN: 'parent-token' };
  await withParentEnvironment(parent, async () => {
    const child = await childEnvironment();
    assert.equal(child.MARIO_FUNCTIONAL_BASE_URL, null);
    assert.equal(child.MARIO_FUNCTIONAL_TOKEN, null);
    for (const [key, value] of Object.entries(parent)) assert.equal(process.env[key], value);
  });
});

test('callers may explicitly target the temporary API and account they created', async () => {
  await withParentEnvironment({ MARIO_FUNCTIONAL_BASE_URL: 'https://parent-target.example.invalid/api', MARIO_FUNCTIONAL_TOKEN: 'parent-token' }, async () => {
    const explicit = { MARIO_FUNCTIONAL_BASE_URL: 'http://127.0.0.1:4217/api', MARIO_FUNCTIONAL_TOKEN: 'temporary-account-token' };
    const child = await childEnvironment(explicit);
    for (const [key, value] of Object.entries(explicit)) assert.equal(child[key], value);
  });
});

test('a disposable database overrides the parent while PostgreSQL service options stay excluded', async () => {
  await withParentEnvironment({ MARIO_TEST_DATABASE_URL: 'postgresql://parent.example.invalid/persistent', PGSERVICE: 'parent-service', PGSERVICEFILE: '/parent/service', PGOPTIONS: '-c search_path=parent' }, async () => {
    const database = 'postgresql://mario_test@127.0.0.1:54321/disposable';
    const child = await childEnvironment({ MARIO_TEST_DATABASE_URL: database, PGSERVICE: 'explicit-service', PGSERVICEFILE: '/explicit/service', PGOPTIONS: '-c search_path=explicit' });
    assert.equal(child.MARIO_TEST_DATABASE_URL, database);
    for (const key of ['PGSERVICE', 'PGSERVICEFILE', 'PGOPTIONS']) assert.equal(child[key], null);
  });
});
