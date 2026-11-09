import assert from 'node:assert/strict';
import test from 'node:test';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { clientSource } from './client-source.mjs';

test('source fingerprints cover tracked descendants, new files and deleted sources while ignoring build artifacts', () => {
  const directory = mkdtempSync(join(tmpdir(), 'mario-client-source-'));
  const git = (...args) => execFileSync('git', args, { cwd: directory, stdio: 'pipe' });
  try {
    git('init');
    git('config', 'user.email', 'fixture@example.test');
    git('config', 'user.name', 'Fixture');
    mkdirSync(join(directory, 'server/src/http'), { recursive: true });
    mkdirSync(join(directory, 'client/dist'), { recursive: true });
    writeFileSync(join(directory, '.gitignore'), 'client/dist/\n');
    const source = join(directory, 'server/src/http/policy.rs');
    writeFileSync(source, 'first');
    git('add', '.');
    git('commit', '-m', 'fixture');
    const first = clientSource(directory);
    assert.equal(first.sourceDirty, false);
    writeFileSync(source, 'second');
    const second = clientSource(directory);
    assert.notEqual(second.sourceFingerprint, first.sourceFingerprint);
    assert.equal(second.sourceDirty, true);
    writeFileSync(join(directory, 'server/src/http/new.rs'), 'new source');
    const third = clientSource(directory);
    assert.notEqual(third.sourceFingerprint, second.sourceFingerprint);
    writeFileSync(join(directory, 'client/dist/ignored.js'), 'build output');
    assert.equal(clientSource(directory).sourceFingerprint, third.sourceFingerprint);
    rmSync(source);
    assert.notEqual(clientSource(directory).sourceFingerprint, third.sourceFingerprint);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
