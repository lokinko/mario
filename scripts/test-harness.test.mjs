import assert from "node:assert/strict";
import test from "node:test";
import {
  assertNoForbiddenLeaks,
  createFaultModelServer,
  createSuite,
} from "./test-harness.mjs";

test("assertNoForbiddenLeaks catches leaked secrets and stack traces", () => {
  assert.doesNotThrow(() =>
    assertNoForbiddenLeaks('{"error":"请求参数无效"}', ["top-secret-key"]),
  );
  assert.throws(
    () =>
      assertNoForbiddenLeaks('{"error":"upstream failed with top-secret-key"}', [
        "top-secret-key",
      ]),
    /leaked forbidden fragment/,
  );
  assert.throws(
    () => assertNoForbiddenLeaks("thread panicked at src/main.rs:10"),
    /leaked forbidden fragment/,
  );
});

test("createFaultModelServer supports queued HTTP errors, malformed JSON, socket aborts, and ok replies", async () => {
  const mock = await createFaultModelServer();
  try {
    mock.enqueue(
      mock.httpError(429, "rate limited"),
      mock.malformedJson("{broken"),
      mock.abortSocket(),
      mock.chatOk("recovered"),
    );

    const r1 = await fetch(`${mock.baseUrl}/chat/completions`, {
      method: "POST",
      headers: { "Content-Type": "application/json", Authorization: "Bearer k1" },
      body: JSON.stringify({ model: "m1" }),
    });
    assert.equal(r1.status, 429);

    const r2 = await fetch(`${mock.baseUrl}/chat/completions`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: "m2" }),
    });
    assert.equal(r2.status, 200);
    assert.equal(await r2.text(), "{broken");

    await assert.rejects(() =>
      fetch(`${mock.baseUrl}/chat/completions`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ model: "m3" }),
      }),
    );

    const r4 = await fetch(`${mock.baseUrl}/chat/completions`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: "m4" }),
    });
    assert.equal(r4.status, 200);
    const json4 = await r4.json();
    assert.equal(json4.choices[0].message.content, "recovered");
    assert.equal(mock.requests.length, 4);
    assert.equal(mock.requests[0].headers.authorization, "Bearer k1");
  } finally {
    await mock.close();
  }
});

test("createSuite runs filtered cases and propagates failures", async () => {
  const executed = [];
  const suite = createSuite("unit-harness", { filter: "second" });
  suite.add("first case", () => {
    executed.push("first");
  });
  suite.add("second case", () => {
    executed.push("second");
    suite.check(true, "invariant holds");
  });
  const summary = await suite.run();
  assert.deepEqual(executed, ["second"]);
  assert.equal(summary.assertions, 1);

  const failing = createSuite("failing-harness");
  let errorHookCalled = false;
  failing.add("broken case", () => {
    throw new Error("boom");
  });
  await assert.rejects(
    () =>
      failing.run({
        onError: () => {
          errorHookCalled = true;
        },
      }),
    /boom/,
  );
  assert.equal(errorHookCalled, true);
});
