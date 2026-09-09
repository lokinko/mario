import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import assert from 'node:assert/strict';
import { test } from 'node:test';

const source = readFileSync(new URL('../site/auth/auth.js', import.meta.url), 'utf8');
function page(hash, replies) {
  const elements = new Map();
  const calls = [];
  const tasks = [];
  let cleaned = false;
  const element = (id) => {
    if (!elements.has(id)) elements.set(id, { hidden: true, value: '', textContent: '', addEventListener(event, handler) { this[event] = handler; } });
    return elements.get(id);
  };
  const context = {
    URLSearchParams, AbortController, setTimeout, clearTimeout,
    location: { hash, pathname: '/mario/auth/' },
    history: { replaceState(_state, _title, url) { assert.equal(url, '/mario/auth/'); cleaned = true; } },
    document: { getElementById: element },
    fetch: async (url, options) => {
      assert.ok(cleaned, 'Credentials must leave browser history before network requests');
      calls.push({ url, options });
      const reply = replies.shift();
      assert.ok(reply, 'Unexpected network request');
      return { ok: reply.ok ?? true, json: async () => reply.body };
    },
  };
  runInNewContext(source, context);
  return { element, calls, tasks, settle: () => new Promise((resolve) => setImmediate(resolve)) };
}

test('confirmation success requires server-confirmed email and clears URL credentials', async () => {
  const p = page('#type=signup&access_token=secret', [{ body: { email: 'user@example.com', email_confirmed_at: '2026-09-09' } }]);
  await p.settle();
  assert.equal(p.element('title').textContent, '邮箱确认成功');
  assert.equal(p.calls[0].options.headers.Authorization, 'Bearer secret');
  assert.equal(p.element('password-form').hidden, true);
});

test('unconfirmed users and expired links never show confirmation success', async () => {
  const p = page('#type=signup&access_token=secret', [{ body: { email: 'user@example.com', email_confirmed_at: null } }]);
  await p.settle();
  assert.notEqual(p.element('title').textContent, '邮箱确认成功');
  assert.match(p.element('error').textContent, /尚未确认/);
  const expired = page('#error_code=otp_expired', []);
  assert.match(expired.element('message').textContent, /过期/);
  assert.equal(expired.calls.length, 0);
});

test('password mismatch stays local; matching passwords update and clear credentials', async () => {
  const p = page('#type=recovery&access_token=secret', [
    { body: { email: 'user@example.com', email_confirmed_at: '2026-09-09' } },
    { body: { id: 'user' } },
  ]);
  await p.settle();
  assert.equal(p.element('password-form').hidden, false);
  p.element('password').value = 'NewPassword2';
  p.element('confirm').value = 'Different3';
  await p.element('password-form').submit({ preventDefault() {} });
  assert.equal(p.calls.length, 1);
  assert.match(p.element('error').textContent, /不一致/);
  p.element('confirm').value = 'NewPassword2';
  await p.element('password-form').submit({ preventDefault() {} });
  assert.equal(p.element('title').textContent, '密码已更新');
  assert.equal(p.element('password').value, '');
  assert.equal(p.element('password-form').hidden, true);
  assert.equal(JSON.parse(p.calls[1].options.body).password, 'NewPassword2');
});

test('hash-token verification requires a click and uses the fixed auth endpoint', async () => {
  const p = page('#type=signup&token_hash=one-time-hash', [
    { body: { access_token: 'verified' } },
    { body: { email: 'user@example.com', email_confirmed_at: '2026-09-09' } },
  ]);
  assert.equal(p.calls.length, 0);
  await p.element('verify').click();
  assert.equal(p.calls[0].url, 'https://haqvpuukgsxuroeqohsk.supabase.co/auth/v1/verify');
  assert.equal(JSON.parse(p.calls[0].options.body).token_hash, 'one-time-hash');
  assert.equal(p.element('title').textContent, '邮箱确认成功');
});
