import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { checkEntrypoints } from './check-entrypoints.mjs';

test('checks npm and full-gate targets before executing builds or database tests', () => {
  const directory = mkdtempSync(join(tmpdir(), 'mario-entrypoints-'));
  try {
    mkdirSync(join(directory, 'scripts'));
    writeFileSync(join(directory, 'package.json'), JSON.stringify({ scripts: { test: 'bash scripts/test.sh', smoke: 'node scripts/smoke.mjs' } }));
    writeFileSync(join(directory, 'scripts/test.sh'), 'node scripts/acceptance.mjs');
    assert.throws(() => checkEntrypoints(directory), /smoke.mjs.*acceptance.mjs/);
    writeFileSync(join(directory, 'scripts/smoke.mjs'), '');
    assert.throws(() => checkEntrypoints(directory), /acceptance.mjs/);
    writeFileSync(join(directory, 'scripts/acceptance.mjs'), '');
    assert.equal(checkEntrypoints(directory), 3);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
