import assert from 'node:assert/strict';
import { test } from 'node:test';
import { workerArguments } from './codex-bridge.mjs';

test('local launch requires the pairing site or its exact API base', () => {
  assert.throws(() => workerArguments([]), /复制 Mario 设置页/);
  assert.throws(() => workerArguments(['--chatgpt-bridge', '--api-url']), /复制 Mario 设置页/);
  assert.deepEqual(workerArguments(['--chatgpt-bridge', 'http://127.0.0.1:4217/']), ['--chatgpt-bridge', 'http://127.0.0.1:4217/']);
  const explicit = ['--chatgpt-bridge', '--api-url', 'https://mario.example.org/deployed/api'];
  assert.deepEqual(workerArguments(explicit), explicit);
  assert.deepEqual(workerArguments(['--chatgpt-auth', 'login', '--new']), ['--chatgpt-auth', 'login', '--new']);
  assert.deepEqual(workerArguments(['--chatgpt-auth', 'login', '--account', 'local-registration']), ['--chatgpt-auth', 'login', '--account', 'local-registration']);
  for (const action of ['status', 'logout']) assert.deepEqual(workerArguments(['--chatgpt-auth', action]), ['--chatgpt-auth', action]);
  assert.throws(() => workerArguments(['--codex-bridge']), /chatgpt:bridge/);
  assert.throws(() => workerArguments(['--codex-bridge', 'https://www.madeagents.ai/mario/']), /chatgpt:bridge/);
});
test('rejects unsafe sites and extra arguments before starting OAuth or building', () => {
  for (const site of ['http://remote.example/', 'https://user:secret@example.com/', 'https://example.com/?token=secret', 'https://example.com/#fragment', 'file:///tmp/site']) {
    assert.throws(() => workerArguments(['--chatgpt-bridge', site]));
    assert.throws(() => workerArguments(['--chatgpt-bridge', '--api-url', site]));
  }
  for (const args of [['--chatgpt-auth'], ['--chatgpt-auth','login','--account'], ['--chatgpt-auth','status','extra'], ['--chatgpt-auth','login','--new','--account','id'], ['--chatgpt-bridge','https://example.com/','extra'], ['--unknown']]) assert.throws(() => workerArguments(args));
});
