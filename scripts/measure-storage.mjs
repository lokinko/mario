import assert from 'node:assert/strict';
import { run, withDisposableCluster } from './postgres-cluster.mjs';

await withDisposableCluster('mario-measure-', async ({ createDatabase }) => {
  const output = await run('cargo', ['test', '--manifest-path', 'server/Cargo.toml', '--lib', 'storage_roundtrip_measurement', '--', '--ignored', '--nocapture'], {
    MARIO_TEST_DATABASE_URL: await createDatabase('mario_measure'),
  }, true);
  const match = output.match(/MARIO_STORAGE_MEASUREMENT:(\{[^\r\n]+\})/);
  assert(match, 'storage measurement did not execute');
  console.log(JSON.stringify(JSON.parse(match[1]), null, 2));
});
