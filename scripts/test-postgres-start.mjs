// PostgreSQL startup failures must expose their server diagnostics to systemd.
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { startPostgres } from './start-common.mjs';

const fixture = mkdtempSync(join(tmpdir(), 'mario-pg-start-'));
const log = join(fixture, 'postgres.log');
try {
  let invoked = false;
  const failure = Error('pg_ctl 失败 (1)');
  await assert.rejects(startPostgres('pg_ctl', fixture, log, 55432, {}, async (_command, argv) => {
    invoked = true;
    const options = argv[argv.indexOf('-o') + 1];
    assert.match(options, /-h 127\.0\.0\.1/);
    assert.match(options, /-p 55432/);
    assert.match(options, /(?:^| )-c unix_socket_directories=(?: |$)/);
    // Simulate a server failure, whose diagnostic is redirected by pg_ctl -l.
    writeFileSync(log, 'OLD LOG\n' + 'x'.repeat(20000) + '\nFATAL: could not create lock file: Permission denied\n');
    throw failure;
  }), error => {
    assert.equal(error.cause, failure);
    assert(error.message.includes(log));
    assert.match(error.message, /FATAL:.*Permission denied/);
    assert(!error.message.includes('OLD LOG'));
    assert(error.message.length < 17000);
    return true;
  });
  assert(invoked);
  await assert.rejects(startPostgres('pg_ctl', fixture, join(fixture, 'missing.log'), 55432, {}, async () => {
    throw failure;
  }), /未能读取日志/);
  let started = false;
  await startPostgres('pg_ctl', fixture, log, 55432, {}, async () => { started = true; });
  assert(started);
  console.log('PASS PostgreSQL startup: loopback TCP only, failure diagnostics, bounded log tail, missing log, successful start');
} finally {
  rmSync(fixture, { recursive: true, force: true });
}
