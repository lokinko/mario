// End-to-end HTTP edge-case regression suite against a DISPOSABLE PostgreSQL database.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { randomBytes, randomUUID } from "node:crypto";
import { once } from "node:events";
import { existsSync, readdirSync } from "node:fs";
import { createServer } from "node:net";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { root, serverBinary } from "./server-artifacts.mjs";

const database = process.env.MARIO_TEST_DATABASE_URL;
assert(
  database,
  "Set MARIO_TEST_DATABASE_URL to a disposable PostgreSQL database",
);

const probe = createServer().listen(0, "127.0.0.1");
await once(probe, "listening");
const port = probe.address().port;
await new Promise((resolve) => probe.close(resolve));

const origin = `http://127.0.0.1:${port}`;
const base = `${origin}/api`;
const masterKey = randomBytes(32).toString("base64");
const webDir = join(root, "client/dist");

let child;
let log = "";
let assertions = 0;

function check(condition, message) {
  assert(condition, message);
  assertions += 1;
}

async function startServer() {
  const env = {
    ...process.env,
    DATABASE_URL: database,
    MARIO_MASTER_KEY: masterKey,
    MARIO_HOST: "127.0.0.1",
    ...(existsSync(join(webDir, "index.html")) ? { MARIO_WEB_DIR: webDir } : {}),
  };
  delete env.MARIO_AGENT_URL;
  child = spawn(serverBinary(), ["--port", String(port)], {
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.on("data", (chunk) => (log += chunk));
  child.stderr.on("data", (chunk) => (log += chunk));
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`${base}/server`)).ok) return;
    } catch {}
    if (child.exitCode !== null) throw Error(log);
    await sleep(50);
  }
  throw Error(`Edge-case test server failed to start:\n${log}`);
}

async function stopServer() {
  if (child?.exitCode === null && child.signalCode === null) {
    const exited = once(child, "exit");
    child.kill("SIGTERM");
    await exited;
  }
}

async function registerUser(tag) {
  const response = await fetch(`${base}/auth/register`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      email: `${tag}-${randomUUID()}@example.test`,
      password: "edge-case-password-0123456789",
    }),
  });
  assert.equal(response.status, 200, await response.clone().text());
  return response.json();
}

async function request(token, path, { method = "GET", body, ifMatch } = {}) {
  const headers = {
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
    ...(body !== undefined ? { "Content-Type": "application/json" } : {}),
    ...(ifMatch !== undefined ? { "If-Match": ifMatch } : {}),
  };
  const response = await fetch(`${base}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  let json;
  if (text && response.headers.get("content-type")?.includes("application/json")) {
    json = JSON.parse(text);
  }
  return {
    status: response.status,
    headers: response.headers,
    text,
    json,
  };
}

try {
  await startServer();

  // 1. Malformed / overlong Authorization headers must always be rejected with 401.
  for (const authHeader of [
    "",
    "Bearer ",
    "Basic dXNlcjpwYXNz",
    `Bearer ${"a".repeat(257)}`,
    "Bearer invalid-token",
  ]) {
    const res = await fetch(`${base}/sync/version`, {
      headers: authHeader ? { Authorization: authHeader } : {},
    });
    assert.equal(res.status, 401, `expected 401 for auth header: ${authHeader.slice(0, 24)}`);
    assertions += 1;
  }

  const userA = await registerUser("edge-a");
  const userB = await registerUser("edge-b");

  // 2. Empty-account FX settings default and lightweight currency update.
  const initialFx = await request(userA.token, "/fx-settings");
  assert.equal(initialFx.status, 200);
  check(initialFx.json.baseCurrency === "CNY", "empty account FX baseCurrency should default to CNY");
  const initialRev = initialFx.headers.get("x-data-revision");
  check(Boolean(initialRev), "GET /api/fx-settings must return x-data-revision");

  // 3. Concurrent lock-free read burst on the same account must all succeed with consistent revision.
  const readPaths = [
    "/snapshot",
    "/sync/version",
    "/fx-settings",
    "/research-evidence",
    "/analyses",
    "/watch",
    "/watch/catalog",
    "/watch/dividend-calendar",
    "/investment-rules",
    "/system-reviews",
    "/review-reminders",
    "/model-config",
  ];
  const readBurst = await Promise.all(
    readPaths.map((path) => request(userA.token, path)),
  );
  for (let i = 0; i < readBurst.length; i++) {
    assert.equal(readBurst[i].status, 200, `concurrent GET ${readPaths[i]} failed: ${readBurst[i].text}`);
    assert.equal(readBurst[i].headers.get("x-data-revision"), initialRev);
    assert.equal(readBurst[i].headers.get("cache-control"), "no-store");
    assertions += 1;
  }

  // 4. Optimistic concurrency: missing/stale If-Match returns 409, and two concurrent writes with the
  // same If-Match must result in exactly one 200 and one 409.
  const baseProfile = readBurst[0].json.profile;
  const missingIfMatch = await request(userA.token, "/profile", {
    method: "PUT",
    body: { ...baseProfile, monthlyIncome: 11111 },
  });
  assert.equal(missingIfMatch.status, 409);
  assertions += 1;

  const staleIfMatch = await request(userA.token, "/profile", {
    method: "PUT",
    ifMatch: "999999",
    body: { ...baseProfile, monthlyIncome: 11111 },
  });
  assert.equal(staleIfMatch.status, 409);
  assertions += 1;

  const [write1, write2] = await Promise.all([
    request(userA.token, "/profile", {
      method: "PUT",
      ifMatch: initialRev,
      body: { ...baseProfile, baseCurrency: "USD", monthlyIncome: 22222 },
    }),
    request(userA.token, "/profile", {
      method: "PUT",
      ifMatch: initialRev,
      body: { ...baseProfile, baseCurrency: "USD", monthlyIncome: 33333 },
    }),
  ]);
  const statuses = [write1.status, write2.status].sort();
  assert.deepEqual(statuses, [200, 409], `expected [200, 409], got ${JSON.stringify(statuses)}`);
  const winner = write1.status === 200 ? write1 : write2;
  let revA = winner.headers.get("x-data-revision");
  check(revA !== initialRev, "winning write must advance x-data-revision");

  const updatedFx = await request(userA.token, "/fx-settings");
  assert.equal(updatedFx.status, 200);
  assert.equal(updatedFx.json.baseCurrency, "USD");
  assertions += 1;

  // 5. Evidence summary omits extractedText while attachment download restores exact bytes,
  // cross-tenant download is rejected, and boundary payloads are validated.
  const attachmentText = "研究报告全文内容：用于校验列表裁剪与附件完整下载。".repeat(40);
  const attachmentBase64 = Buffer.from(attachmentText, "utf8").toString("base64");
  const createdEvidence = await request(userA.token, "/research-evidence", {
    method: "POST",
    ifMatch: revA,
    body: {
      content: "核心要点笔记",
      assetName: "招商银行 600036.SS",
      category: "年报",
      attachments: [{ name: "report.txt", dataBase64: attachmentBase64 }],
    },
  });
  assert.equal(createdEvidence.status, 200, createdEvidence.text);
  revA = createdEvidence.headers.get("x-data-revision");
  const evidenceId = createdEvidence.json.id;
  const attachmentId = createdEvidence.json.attachments[0].id;

  const listedEvidence = await request(userA.token, "/research-evidence");
  assert.equal(listedEvidence.status, 200);
  const listedItem = listedEvidence.json.find((item) => item.id === evidenceId);
  check(Boolean(listedItem), "saved evidence must appear in list");
  assert.equal(listedItem.attachments[0].name, "report.txt");
  assert.equal(listedItem.attachments[0].extractedText ?? "", "");
  assert.equal(listedItem.attachments[0].dataBase64, undefined);
  check(
    !JSON.stringify(listedItem).includes("第一行研究文本"),
    "evidence summary list must never leak extracted attachment payload",
  );
  assertions += 1;

  const downloaded = await request(
    userA.token,
    `/research-evidence/${evidenceId}/attachments/${attachmentId}`,
  );
  assert.equal(downloaded.status, 200);
  assert.equal(
    Buffer.from(downloaded.json.dataBase64, "base64").toString("utf8"),
    attachmentText,
  );
  assertions += 1;

  const crossTenantDownload = await request(
    userB.token,
    `/research-evidence/${evidenceId}/attachments/${attachmentId}`,
  );
  assert.equal(crossTenantDownload.status, 404);
  assertions += 1;

  for (const invalidEvidence of [
    { content: "   \n\t  " },
    { content: "字".repeat(20_001) },
    { content: "bad b64", attachments: [{ name: "bad.txt", dataBase64: "%%%invalid%%%" }] },
  ]) {
    const res = await request(userA.token, "/research-evidence", {
      method: "POST",
      ifMatch: revA,
      body: invalidEvidence,
    });
    assert.equal(res.status, 400, `expected 400 for invalid evidence: ${res.text}`);
    assertions += 1;
  }

  // 6. Investment rules, watch symbols, and dividend calendar validation edge cases.
  const blankRule = await request(userA.token, "/investment-rules", {
    method: "POST",
    ifMatch: revA,
    body: {
      category: "   ",
      statement: "   ",
      trigger: "\t",
      rationale: "\n",
      active: true,
    },
  });
  assert.equal(blankRule.status, 400);
  assertions += 1;

  for (const badSymbol of ["", "INVALID!", "600036.SS'; DROP TABLE watch_entries;--"]) {
    const res = await request(userA.token, "/watch", {
      method: "PUT",
      ifMatch: revA,
      body: { symbol: badSymbol, name: "异常测试", thesis: "" },
    });
    assert.equal(res.status, 400, `expected 400 for bad symbol: ${badSymbol}`);
    assertions += 1;
  }
  const missingObs = await request(userA.token, "/watch/600036.SS/observation");
  assert.equal(missingObs.status, 404);
  assertions += 1;

  const badCalendarDate = await request(
    userA.token,
    "/watch/dividend-calendar?from=2026-02-30",
  );
  assert.equal(badCalendarDate.status, 400);
  const leapCalendar = await request(
    userA.token,
    "/watch/dividend-calendar?scope=all&from=2024-02-29",
  );
  assert.equal(leapCalendar.status, 200);
  assert.equal(leapCalendar.json.start, "2024-02-29");
  assertions += 2;

  // 7. Immediate session revocation on logout without affecting other tenants.
  const warmCheck = await request(userA.token, "/sync/version");
  assert.equal(warmCheck.status, 200);
  const logoutRes = await request(userA.token, "/auth/logout", { method: "POST" });
  assert.equal(logoutRes.status, 200);
  const afterLogout = await request(userA.token, "/sync/version");
  assert.equal(afterLogout.status, 401);
  // User B session remains completely unaffected.
  const userBStillValid = await request(userB.token, "/sync/version");
  assert.equal(userBStillValid.status, 200);
  assertions += 3;

  // 8. Built asset split & Cache-Control boundary check when client/dist is present.
  if (existsSync(join(webDir, "assets"))) {
    const files = readdirSync(join(webDir, "assets"));
    const reactChunk = files.find((f) => /^react-vendor-.*\.js$/.test(f));
    const antdChunk = files.find((f) => /^antd-core-.*\.js$/.test(f));
    check(Boolean(reactChunk), "client/dist/assets must contain split react-vendor chunk");
    check(Boolean(antdChunk), "client/dist/assets must contain split antd-core chunk");

    const entryRes = await fetch(`${origin}/`);
    assert.equal(entryRes.status, 200);
    assert.equal(entryRes.headers.get("cache-control"), "no-cache");
    const assetRes = await fetch(`${origin}/assets/${reactChunk}`);
    assert.equal(assetRes.status, 200);
    assert.equal(
      assetRes.headers.get("cache-control"),
      "public, max-age=31536000, immutable",
    );
    const missingAssetRes = await fetch(`${origin}/assets/nonexistent-chunk.js`);
    assert.equal(missingAssetRes.status, 404);
    assert.equal(missingAssetRes.headers.get("cache-control"), "no-store");
    assertions += 3;
  }

  console.log(
    `PASS edge cases: ${assertions} assertions (auth bounds, lock-free read burst, optimistic write race, evidence summary vs download, validation fuzzing, immediate logout cache eviction, asset caching)`,
  );
} finally {
  await stopServer();
}
