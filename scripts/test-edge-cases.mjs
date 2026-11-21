// Adversarial end-to-end HTTP regression & fault-injection suite against a DISPOSABLE PostgreSQL database.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { root } from "./server-artifacts.mjs";
import {
  ApiAccount,
  assertNoForbiddenLeaks,
  createFaultModelServer,
  createSuite,
  startMarioServer,
} from "./test-harness.mjs";

const database = process.env.MARIO_TEST_DATABASE_URL;
assert(
  database,
  "Set MARIO_TEST_DATABASE_URL to a disposable PostgreSQL database",
);

const webDir = join(root, "client/dist");
const today = new Date().toISOString().slice(0, 10);
const yesterday = new Date(Date.now() - 86400000).toISOString().slice(0, 10);
const twoDaysAgo = new Date(Date.now() - 2 * 86400000).toISOString().slice(0, 10);

const server = await startMarioServer({ database, webDir });
const faultModel = await createFaultModelServer();
const forbiddenGlobals = [server.masterKey];
const suite = createSuite("adversarial-edge-cases");

function cashHolding(name, amount, currency = "CNY", valuationDate = twoDaysAgo) {
  return {
    symbol: "",
    name,
    assetClass: "现金",
    marketValue: amount,
    costBasis: 0,
    targetPct: 0,
    currency,
    fxRateToBase: null,
    valuationDate,
    fxRateSource: "",
    fxRateObservedOn: "",
  };
}

// ---------------------------------------------------------------------------
// Suite 1: Auth boundaries, malformed payloads, path traversal, logout eviction
// ---------------------------------------------------------------------------
suite.add("auth-security-and-malformed-input", async () => {
  // 1a. Malformed / overlong / wrong-scheme Authorization headers must return 401.
  for (const authHeader of [
    "",
    "Bearer ",
    "Basic dXNlcjpwYXNz",
    `Bearer ${"a".repeat(257)}`,
    "Bearer invalid-token-value",
  ]) {
    const res = await fetch(`${server.base}/sync/version`, {
      headers: authHeader ? { Authorization: authHeader } : {},
    });
    const text = await res.text();
    assert.equal(res.status, 401, `expected 401 for auth header: ${authHeader.slice(0, 24)}`);
    assertNoForbiddenLeaks(text, forbiddenGlobals);
    suite.count();
  }

  // 1b. Registration & login negative boundaries (short password, duplicate email, case normalization, wrong password).
  const email = `Auth-Case-${randomUUID()}@Example.Test`;
  const shortPwd = await fetch(`${server.base}/auth/register`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ email, password: "short" }),
  });
  assert.equal(shortPwd.status, 400);
  suite.count();

  const userA = await ApiAccount.register(server.base, {
    email,
    password: "strong-password-0123456789",
    secrets: forbiddenGlobals,
  });
  const userB = await ApiAccount.register(server.base, {
    tag: "auth-b",
    secrets: forbiddenGlobals,
  });

  // Duplicate registration with altered case & surrounding spaces must be rejected.
  const dup = await fetch(`${server.base}/auth/register`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      email: `  ${email.toLowerCase()}  `,
      password: "another-strong-password-12345",
    }),
  });
  assert.equal(dup.status, 400, "duplicate email (case/whitespace normalized) must fail");
  suite.count();

  // Wrong password login fails with 401 and leaks no hash/secret.
  const wrongLogin = await fetch(`${server.base}/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ email, password: "wrong-password-0123456789" }),
  });
  assert.equal(wrongLogin.status, 401);
  assertNoForbiddenLeaks(await wrongLogin.text(), [...forbiddenGlobals, "argon2"]);
  suite.count();

  // 1c. Malformed JSON syntax and wrong Content-Type must be rejected without mutating revision.
  await userA.assertRejectedWithoutMutation(
    "/profile",
    { method: "PUT", rawBody: "{broken json", contentType: "application/json" },
    [400, 422],
  );
  await userA.assertRejectedWithoutMutation(
    "/profile",
    { method: "PUT", rawBody: "plain-text", contentType: "text/plain" },
    [400, 415],
  );
  suite.count(2);

  // 1d. Path traversal & private file access must never succeed; unknown /api/* must return 404.
  for (const path of [
    "/.env",
    "/%2e%2e/.env",
    "/assets/../../.env",
    "/mario.db",
    "/api/does-not-exist-route",
  ]) {
    const res = await fetch(`${server.origin}${path}`, {
      headers: { Authorization: `Bearer ${userA.token}` },
    });
    assert.equal(res.status, 404, `expected 404 for ${path}, got ${res.status}`);
    suite.count();
  }

  // 1e. Immediate logout revocation blocks both reads and writes for userA while leaving userB intact.
  const snapA = (await userA.expectOk("/snapshot")).json;
  await userA.expectOk("/auth/logout", { method: "POST", body: {} });
  const afterLogoutRead = await userA.rawRequest("/sync/version");
  assert.equal(afterLogoutRead.status, 401);
  const afterLogoutWrite = await userA.rawRequest("/profile", {
    method: "PUT",
    ifMatch: userA.revision,
    body: { ...snapA.profile, monthlyIncome: 99999 },
  });
  assert.equal(afterLogoutWrite.status, 401);
  assert.equal((await userB.rawRequest("/sync/version")).status, 200);
  suite.count(3);
});

// ---------------------------------------------------------------------------
// Suite 2: Cross-tenant IDOR matrix (read/write/delete/receipts/daily-review)
// ---------------------------------------------------------------------------
suite.add("cross-tenant-idor-matrix", async () => {
  const owner = await ApiAccount.register(server.base, {
    tag: "idor-owner",
    secrets: forbiddenGlobals,
  });
  const attacker = await ApiAccount.register(server.base, {
    tag: "idor-attacker",
    secrets: forbiddenGlobals,
  });

  // Seed diverse resources under owner.
  const cashSnap = (
    await owner.expectOk("/holdings", {
      method: "POST",
      body: cashHolding("主账户存款", 50000),
    })
  ).json;
  const cashId = cashSnap.holdings[0].id;
  const tradeReq = {
    requestId: randomUUID(),
    holdingId: "",
    expectedRevision: null,
    newHolding: { name: "跨租户测试股", symbol: "", assetClass: "股票", currency: "CNY" },
    kind: "buy",
    occurredOn: twoDaysAgo,
    sequence: 1,
    quantity: "100",
    price: "20",
    grossAmount: "2000",
    fees: "5",
    taxes: "0",
    account: "券商A",
    externalId: "idor-trade-1",
    note: "",
  };
  const afterTrade = (
    await owner.expectOk("/holding-trades", { method: "POST", body: tradeReq })
  ).json;
  const stockId = afterTrade.holdingTrades[0].holdingId;
  const tradeId = afterTrade.holdingTrades[0].id;

  const evidence = (
    await owner.expectOk("/research-evidence", {
      method: "POST",
      body: {
        content: "绝密研究笔记-IDOR",
        assetName: "测试标的",
        category: "研报",
        attachments: [
          {
            name: "secret.txt",
            dataBase64: Buffer.from("OWNER_SECRET_BYTES_42", "utf8").toString("base64"),
          },
        ],
      },
    })
  ).json;
  const attachmentId = evidence.attachments[0].id;

  await owner.expectOk("/daily-assets/ensure", {
    method: "POST",
    body: { timezone: "Asia/Shanghai" },
  });
  const reviewRun = (
    await owner.expectOk("/daily-review/run", {
      method: "POST",
      body: { reviewDay: yesterday },
    })
  ).json;
  const reviewItemId = reviewRun.feed.items[0]?.id;
  suite.check(Boolean(reviewItemId), "owner daily review must produce at least one deterministic item");

  const ownerRevBefore = (await owner.syncVersion()).revision;

  // Attacker attempts direct access/mutation on every owner resource ID.
  await attacker.assertRejectedWithoutMutation(
    `/holdings/${cashId}`,
    { method: "PUT", body: cashHolding("被篡改", 1) },
    [400, 404],
    { targetAccount: owner },
  );
  await attacker.assertRejectedWithoutMutation(
    `/holdings/${cashId}/amount`,
    {
      method: "PUT",
      body: {
        amount: 0,
        expectedRevision: afterTrade.holdingRevisions[cashId],
        requestId: randomUUID(),
      },
    },
    [400, 404, 409],
    { targetAccount: owner },
  );
  await attacker.assertRejectedWithoutMutation(
    `/holdings/${stockId}`,
    {
      method: "DELETE",
      body: { expectedRevision: afterTrade.holdingRevisions[stockId] },
    },
    [400, 404, 409],
    { targetAccount: owner },
  );
  // DELETE /holding-trades/{id} is idempotent on missing IDs in the caller's tenant:
  // it may return 200 with the attacker's own empty snapshot, but MUST NEVER leak or alter owner's trade.
  const foreignTradeDelete = await attacker.expectOk(`/holding-trades/${tradeId}`, {
    method: "DELETE",
    body: { expectedRevision: afterTrade.holdingRevisions[stockId] },
  });
  assert.equal(foreignTradeDelete.json.holdingTrades.length, 0);
  assert.equal(foreignTradeDelete.json.holdings.length, 0);
  assert.equal((await owner.syncVersion()).revision, ownerRevBefore);
  assert.equal(
    (await owner.expectOk("/snapshot")).json.holdingTrades.some((t) => t.id === tradeId),
    true,
    "cross-tenant idempotent trade delete must never affect owner's trade",
  );
  await attacker.assertRejectedWithoutMutation(
    `/research-evidence/${evidence.id}/attachments/${attachmentId}`,
    { method: "GET" },
    [404],
    { targetAccount: owner, extraForbidden: ["OWNER_SECRET_BYTES_42"] },
  );
  await attacker.assertRejectedWithoutMutation(
    `/research-evidence/${evidence.id}/status`,
    { method: "PUT", body: { active: false } },
    [400, 404],
    { targetAccount: owner },
  );
  await attacker.assertRejectedWithoutMutation(
    "/daily-review/read",
    { method: "POST", body: { id: reviewItemId } },
    [404],
    { targetAccount: owner },
  );
  const foreignResolve = (
    await attacker.expectOk("/financial-operations/resolve", {
      method: "POST",
      body: { method: "POST", path: "/api/holding-trades", input: tradeReq },
    })
  ).json;
  assert.deepEqual(foreignResolve, { committed: false, snapshot: null });
  suite.count(9);

  // Owner revision and data remain 100% intact after all cross-tenant attacks.
  assert.equal((await owner.syncVersion()).revision, ownerRevBefore);
  const ownerSnapAfter = (await owner.expectOk("/snapshot")).json;
  assert.equal(ownerSnapAfter.holdings.find((h) => h.id === cashId).marketValue, 50000);
  const ownerFeedAfter = (await owner.expectOk("/daily-review")).json;
  assert.equal(
    ownerFeedAfter.items.find((i) => i.id === reviewItemId).readAt,
    null,
    "cross-tenant read attempt must never mark owner item as read",
  );
  suite.count(3);
});

// ---------------------------------------------------------------------------
// Suite 3: Optimistic locking, concurrent read/write races, idempotency conflicts
// ---------------------------------------------------------------------------
suite.add("optimistic-locking-idempotency-and-races", async () => {
  const user = await ApiAccount.register(server.base, {
    tag: "race",
    secrets: forbiddenGlobals,
  });
  const initialRev = user.revision;

  // 3a. Concurrent lock-free read burst across all major read endpoints (including /daily-review).
  const readPaths = [
    "/snapshot",
    "/sync/version",
    "/fx-settings",
    "/research-evidence",
    "/analyses",
    "/watch",
    "/watch/catalog",
    "/watch/dividend-calendar",
    "/model-config",
    "/daily-review",
  ];
  const burst = await Promise.all(readPaths.map((p) => user.rawRequest(p)));
  for (let i = 0; i < burst.length; i += 1) {
    assert.equal(burst[i].status, 200, `concurrent GET ${readPaths[i]} failed: ${burst[i].text}`);
    assert.equal(burst[i].headers.get("x-data-revision"), initialRev);
    assert.equal(burst[i].headers.get("cache-control"), "no-store");
    suite.count();
  }

  // 3b. Missing / stale / non-numeric If-Match rejected with 409 without mutating revision.
  const baseProfile = burst[0].json.profile;
  for (const badIfMatch of [null, "999999", "-1", "not-a-rev"]) {
    await user.assertRejectedWithoutMutation(
      "/profile",
      {
        method: "PUT",
        ifMatch: badIfMatch,
        body: { ...baseProfile, monthlyIncome: 11111 },
      },
      [409],
    );
    suite.count();
  }

  // 3c. 4-way concurrent write race on /profile with same If-Match: strictly 1 wins, 3 get 409.
  const raceRev = user.revision;
  const profileRace = await Promise.all(
    [21000, 22000, 23000, 24000].map((monthlyIncome) =>
      user.rawRequest("/profile", {
        method: "PUT",
        ifMatch: raceRev,
        body: { ...baseProfile, baseCurrency: "USD", monthlyIncome },
      }),
    ),
  );
  assert.deepEqual(
    profileRace.map((r) => r.status).sort(),
    [200, 409, 409, 409],
  );
  const winningIncome = profileRace.find((r) => r.status === 200).json.profile.monthlyIncome;
  assert.equal((await user.expectOk("/snapshot")).json.profile.monthlyIncome, winningIncome);
  assert.equal((await user.expectOk("/fx-settings")).json.baseCurrency, "USD");
  suite.count(3);

  // 3d. Concurrent amount updates on the same cash holding + idempotent replay vs tampered replay.
  const snap = (
    await user.expectOk("/holdings", {
      method: "POST",
      body: cashHolding("竞态现金", 10000, "USD"),
    })
  ).json;
  const cashId = snap.holdings[0].id;
  const hRev = snap.holdingRevisions[cashId];
  const gRev = user.revision;

  const amountRace = await Promise.all(
    [10001, 10002, 10003].map((amount) =>
      user.rawRequest(`/holdings/${cashId}/amount`, {
        method: "PUT",
        ifMatch: gRev,
        body: { amount, expectedRevision: hRev, requestId: randomUUID() },
      }),
    ),
  );
  assert.deepEqual(
    amountRace.map((r) => r.status).sort(),
    [200, 409, 409],
  );
  suite.count();

  // Exact idempotent replay succeeds with same result and does not advance revision;
  // tampered payload with same requestId MUST return 409!
  const freshSnap = (await user.expectOk("/snapshot")).json;
  const idemReq = {
    amount: 12345,
    expectedRevision: freshSnap.holdingRevisions[cashId],
    requestId: randomUUID(),
  };
  const staleGlobalRev = user.revision;
  const firstCommit = await user.expectOk(`/holdings/${cashId}/amount`, {
    method: "PUT",
    ifMatch: staleGlobalRev,
    body: idemReq,
  });
  const committedRev = user.revision;
  const replay = await user.expectOk(`/holdings/${cashId}/amount`, {
    method: "PUT",
    ifMatch: staleGlobalRev,
    body: idemReq,
  });
  assert.equal(replay.json.holdings[0].marketValue, 12345);
  assert.equal((await user.syncVersion()).revision, committedRev);

  await user.assertRejectedWithoutMutation(
    `/holdings/${cashId}/amount`,
    {
      method: "PUT",
      ifMatch: staleGlobalRev,
      body: { ...idemReq, amount: 99999 },
    },
    [409],
  );
  assert.equal(firstCommit.json.holdings[0].marketValue, 12345);
  suite.count(3);
});

// ---------------------------------------------------------------------------
// Suite 4: Financial ledger adversarial invariants (oversell, negative values,
// history tampering, missing FX comparability, option-anchor deletion guard)
// ---------------------------------------------------------------------------
suite.add("financial-ledger-adversarial-invariants", async () => {
  const user = await ApiAccount.register(server.base, {
    tag: "ledger",
    secrets: forbiddenGlobals,
  });

  // 4a. Multi-currency without FX rate MUST NOT naively sum USD into CNY.
  await user.expectOk("/holdings", {
    method: "POST",
    body: cashHolding("人民币存款", 10000, "CNY"),
  });
  const unpricedFxSnap = (
    await user.expectOk("/holdings", {
      method: "POST",
      body: cashHolding("美元存款", 1000, "USD"),
    })
  ).json;
  // Depending on whether automatic ECB fetch is reachable or not, verify FX invariants strictly:
  const usdHolding = unpricedFxSnap.holdings.find((h) => h.currency === "USD");
  if (!unpricedFxSnap.valuationStatus.comparable) {
    assert.equal(usdHolding.fxRateToBase, null);
  }
  await user.expectOk("/fx-settings", {
    method: "PUT",
    body: { currency: "USD", rate: 7.2 },
  });
  const pricedFxSnap = (await user.expectOk("/snapshot")).json;
  assert.equal(pricedFxSnap.valuationStatus.comparable, true);
  assert.equal(pricedFxSnap.totalValue, 17200);
  suite.count(2);

  // 4b. Open a stock position with 10 shares, then bombard with invalid/oversell trades.
  const buyReq = {
    requestId: randomUUID(),
    holdingId: "",
    expectedRevision: null,
    newHolding: { name: "账本对抗测试股", symbol: "", assetClass: "股票", currency: "CNY" },
    kind: "buy",
    occurredOn: twoDaysAgo,
    sequence: 1,
    quantity: "10",
    price: "100",
    grossAmount: "1000",
    fees: "5",
    taxes: "0",
    account: "对抗券商",
    externalId: "adv-buy-1",
    note: "",
  };
  let snap = (await user.expectOk("/holding-trades", { method: "POST", body: buyReq })).json;
  const stockId = snap.holdingTrades[0].holdingId;
  const buyTradeId = snap.holdingTrades[0].id;
  const stockRev = () => snap.holdingRevisions[stockId];

  const badTrades = [
    // Oversell: selling 15 shares when holding only 10
    {
      label: "oversell",
      body: {
        ...buyReq,
        requestId: randomUUID(),
        holdingId: stockId,
        newHolding: null,
        expectedRevision: stockRev(),
        kind: "sell",
        occurredOn: yesterday,
        sequence: 2,
        quantity: "15",
        price: "110",
        grossAmount: "1650",
        externalId: "adv-oversell",
      },
      statuses: [400],
    },
    // Negative quantity
    {
      label: "negative-quantity",
      body: {
        ...buyReq,
        requestId: randomUUID(),
        holdingId: stockId,
        newHolding: null,
        expectedRevision: stockRev(),
        quantity: "-5",
        grossAmount: "500",
      },
      statuses: [400],
    },
    // Zero quantity buy
    {
      label: "zero-buy",
      body: {
        ...buyReq,
        requestId: randomUUID(),
        holdingId: stockId,
        newHolding: null,
        expectedRevision: stockRev(),
        quantity: "0",
        grossAmount: "0",
      },
      statuses: [400],
    },
    // Negative fees
    {
      label: "negative-fees",
      body: {
        ...buyReq,
        requestId: randomUUID(),
        holdingId: stockId,
        newHolding: null,
        expectedRevision: stockRev(),
        fees: "-1",
      },
      statuses: [400],
    },
    // Impossible calendar date
    {
      label: "bad-date",
      body: {
        ...buyReq,
        requestId: randomUUID(),
        holdingId: stockId,
        newHolding: null,
        expectedRevision: stockRev(),
        occurredOn: "2026-02-30",
      },
      statuses: [400],
    },
    // Unknown extra field (deny_unknown_fields protection)
    {
      label: "unknown-field",
      body: {
        ...buyReq,
        requestId: randomUUID(),
        holdingId: stockId,
        newHolding: null,
        expectedRevision: stockRev(),
        injectedField: "evil",
      },
      statuses: [400, 422],
    },
  ];
  for (const item of badTrades) {
    await user.assertRejectedWithoutMutation(
      "/holding-trades",
      { method: "POST", body: item.body },
      item.statuses,
    );
    suite.count();
  }

  // 4c. Sell 6 shares validly, then try deleting the initial 10-share buy (would cause -6 shares).
  snap = (
    await user.expectOk("/holding-trades", {
      method: "POST",
      body: {
        ...buyReq,
        requestId: randomUUID(),
        holdingId: stockId,
        newHolding: null,
        expectedRevision: stockRev(),
        kind: "sell",
        occurredOn: yesterday,
        sequence: 2,
        quantity: "6",
        price: "120",
        grossAmount: "720",
        fees: "2",
        taxes: "1",
        externalId: "adv-sell-6",
      },
    })
  ).json;
  await user.assertRejectedWithoutMutation(
    `/holding-trades/${buyTradeId}`,
    { method: "DELETE", body: { expectedRevision: stockRev() } },
    [400],
  );
  assert.equal((await user.expectOk("/snapshot")).json.holdingTradeSummaries[stockId].quantity, "4");
  suite.count(2);

  // 4d. Linked stock option prevents deleting its underlying stock anchor.
  snap = (
    await user.expectOk("/holding-trades", {
      method: "POST",
      body: {
        requestId: randomUUID(),
        holdingId: "",
        expectedRevision: null,
        newHolding: {
          name: "绑定认沽期权",
          symbol: "",
          assetClass: "期权",
          currency: "CNY",
          option: {
            underlyingId: stockId,
            account: "对抗券商",
            optionType: "put",
            side: "short",
            strike: "90",
            expiresOn: today,
            multiplier: "100",
          },
        },
        kind: "sell",
        occurredOn: yesterday,
        sequence: 3,
        quantity: "1",
        price: "2",
        grossAmount: "200",
        fees: "1",
        taxes: "0",
        account: "对抗券商",
        externalId: "adv-opt-1",
        note: "",
      },
    })
  ).json;
  await user.assertRejectedWithoutMutation(
    `/holdings/${stockId}`,
    { method: "DELETE", body: { expectedRevision: snap.holdingRevisions[stockId] } },
    [400],
  );
  suite.count();
});

// ---------------------------------------------------------------------------
// Suite 5: Evidence attachment truncation, leak guard (fixes former false-green check!),
// validation bounds, and watch/calendar input fuzzing
// ---------------------------------------------------------------------------
suite.add("evidence-attachments-leak-and-validation-fuzzing", async () => {
  const user = await ApiAccount.register(server.base, {
    tag: "evidence",
    secrets: forbiddenGlobals,
  });

  const sentinel = "SENTINEL_EXTRACT_SECRET_98765";
  const attachmentText = `${sentinel}：用于真实检验列表裁剪与附件完整字节下载。`.repeat(30);
  const created = (
    await user.expectOk("/research-evidence", {
      method: "POST",
      body: {
        content: "公开摘要标题",
        assetName: "招商银行 600036.SS",
        category: "年报",
        attachments: [
          {
            name: "report.txt",
            dataBase64: Buffer.from(attachmentText, "utf8").toString("base64"),
          },
        ],
      },
    })
  ).json;
  const attachmentId = created.attachments[0].id;

  const listed = (await user.expectOk("/research-evidence")).json;
  const item = listed.find((e) => e.id === created.id);
  suite.check(Boolean(item), "created evidence must appear in list");
  assert.equal(item.attachments[0].extractedText ?? "", "");
  assert.equal(item.attachments[0].dataBase64, undefined);
  suite.check(
    !JSON.stringify(item).includes(sentinel),
    "evidence summary list must NEVER leak extracted attachment text sentinel",
  );

  const downloaded = (
    await user.expectOk(`/research-evidence/${created.id}/attachments/${attachmentId}`)
  ).json;
  assert.equal(
    Buffer.from(downloaded.dataBase64, "base64").toString("utf8"),
    attachmentText,
  );
  suite.count(3);

  // Invalid evidence payloads must fail with 400 and leave revision untouched.
  for (const invalidEvidence of [
    { content: "   \n\t  " },
    { content: "字".repeat(20_001) },
    { content: "bad b64", attachments: [{ name: "bad.txt", dataBase64: "%%%invalid%%%" }] },
  ]) {
    await user.assertRejectedWithoutMutation(
      "/research-evidence",
      { method: "POST", body: invalidEvidence },
      [400],
    );
    suite.count();
  }

  // Retired legacy review routes, SQL-injection-like watch symbols, and invalid calendar dates.
  await user.assertRejectedWithoutMutation("/investment-rules", { method: "GET" }, [404]);
  for (const badSymbol of ["", "INVALID!", "600036.SS'; DROP TABLE watch_entries;--"]) {
    await user.assertRejectedWithoutMutation(
      "/watch",
      { method: "PUT", body: { symbol: badSymbol, name: "异常测试", thesis: "" } },
      [400],
    );
    suite.count();
  }
  await user.assertRejectedWithoutMutation("/watch/600036.SS/observation", { method: "GET" }, [404]);
  await user.assertRejectedWithoutMutation("/watch/dividend-calendar?from=2026-02-30", { method: "GET" }, [400]);
  const leap = (await user.expectOk("/watch/dividend-calendar?scope=all&from=2024-02-29")).json;
  assert.equal(leap.start, "2024-02-29");
  suite.count(4);
});

// ---------------------------------------------------------------------------
// Suite 6: AI upstream fault injection, privacy/secret guard, history non-pollution
// ---------------------------------------------------------------------------
suite.add("ai-fault-injection-and-history-non-pollution", async () => {
  const user = await ApiAccount.register(server.base, {
    tag: "ai-fault",
    secrets: forbiddenGlobals,
  });
  const modelKey = "sk-adversarial-secret-key-never-leak";
  user.secrets.push(modelKey);

  await user.expectOk("/model-config", {
    method: "PUT",
    body: {
      provider: "openai-chat",
      baseUrl: faultModel.baseUrl,
      model: "fault-test-model",
      apiKey: modelKey,
    },
  });

  const baseReq = {
    question: "请做一次组合风险排查",
    workflow: "council",
    webSearch: false,
    contextSelection: { includeProfile: false, includeHoldings: false, includeEvidence: false },
  };
  const preview = (
    await user.expectOk("/analysis/preview", { method: "POST", body: baseReq })
  ).json;

  // 6a. Stale previewRevision MUST be rejected BEFORE any upstream model call is made.
  faultModel.reset();
  await user.assertRejectedWithoutMutation(
    "/analysis",
    { method: "POST", body: { ...baseReq, previewRevision: "stale-preview-rev" } },
    [400],
  );
  assert.equal(faultModel.requests.length, 0, "stale preview must not invoke upstream model");
  suite.count(2);

  // 6b. Inject upstream 500, 429, malformed JSON, and abrupt socket reset.
  const faults = [
    { name: "upstream-500", responder: () => faultModel.httpError(500, "internal explosion") },
    { name: "upstream-429", responder: () => faultModel.httpError(429, "rate limit hit") },
    { name: "malformed-json", responder: () => faultModel.malformedJson("{not-json-at-all") },
    { name: "socket-abort", responder: () => faultModel.abortSocket() },
  ];
  for (const fault of faults) {
    faultModel.reset();
    faultModel.setDefault(fault.responder);
    const res = await user.assertRejectedWithoutMutation(
      "/analysis",
      { method: "POST", body: { ...baseReq, previewRevision: preview.contextRevision } },
      [429, 502, 504],
      { extraForbidden: [modelKey] },
    );
    suite.check(
      faultModel.requests.length >= 1,
      `${fault.name} should have reached mock upstream`,
    );
    assert.equal(
      (await user.expectOk("/analyses")).json.length,
      0,
      `${fault.name} must never save partial/failed analysis to history (${res.status})`,
    );
    suite.count();
  }

  // 6c. Stream endpoint (/analysis/stream) under upstream 500 must emit explicit SSE error event and keep history empty.
  faultModel.reset();
  faultModel.setDefault(() => faultModel.httpError(500, "stream boom"));
  const streamRes = await user.rawRequest("/analysis/stream", {
    method: "POST",
    useRevision: true,
    body: { ...baseReq, previewRevision: preview.contextRevision },
  });
  assert.equal(streamRes.status, 200);
  assertNoForbiddenLeaks(streamRes.text, [...forbiddenGlobals, modelKey]);
  const events = streamRes.text
    .split("\n")
    .filter((line) => line.startsWith("data: "))
    .map((line) => JSON.parse(line.slice(6)));
  suite.check(
    events.some((e) => e.type === "error"),
    `SSE stream must emit an error event on upstream failure, got: ${JSON.stringify(events)}`,
  );
  suite.check(
    !events.some((e) => e.type === "complete"),
    "SSE stream must never emit complete event on upstream failure",
  );
  assert.equal((await user.expectOk("/analyses")).json.length, 0);
  suite.count(3);
});

// ---------------------------------------------------------------------------
// Suite 7: Daily automated review degradation, JSON repair, read preservation, validation
// ---------------------------------------------------------------------------
suite.add("daily-review-degradation-repair-and-validation", async () => {
  const user = await ApiAccount.register(server.base, {
    tag: "daily-review",
    secrets: forbiddenGlobals,
  });

  // Seed holdings + daily snapshot so deterministic cards are generated.
  await user.expectOk("/holdings", {
    method: "POST",
    body: cashHolding("复盘测试存款", 88000),
  });
  await user.expectOk("/daily-assets/ensure", {
    method: "POST",
    body: { timezone: "Asia/Shanghai" },
  });

  // 7a. Invalid settings & query parameters must be rejected without mutating revision.
  for (const badTime of ["25:00", "7:30", "07:60", "abc", ""]) {
    await user.assertRejectedWithoutMutation(
      "/daily-review/settings",
      {
        method: "PUT",
        body: { enabled: true, scheduleTime: badTime, scope: "holdingsAndWatch", webSearch: false },
      },
      [400],
    );
    suite.count();
  }
  await user.assertRejectedWithoutMutation(
    "/daily-review/settings",
    {
      method: "PUT",
      body: { enabled: true, scheduleTime: "07:30", scope: "invalidScope", webSearch: false },
    },
    [400, 422],
  );
  await user.assertRejectedWithoutMutation("/daily-review?day=2026-02-30", { method: "GET" }, [400]);
  await user.assertRejectedWithoutMutation("/daily-review?kind=forgedKind", { method: "GET" }, [400]);
  await user.assertRejectedWithoutMutation(
    "/daily-review/run",
    { method: "POST", body: { reviewDay: "2026-02-30" } },
    [400],
  );
  await user.assertRejectedWithoutMutation(
    "/daily-review/read",
    { method: "POST", body: {} },
    [400],
  );
  await user.assertRejectedWithoutMutation(
    "/daily-review/read",
    { method: "POST", body: { id: "nonexistent-review-item" } },
    [404],
  );
  suite.count(6);

  // 7b. Without any model configured, manual run degrades gracefully while preserving deterministic cards.
  const noModelRun = (
    await user.expectOk("/daily-review/run", {
      method: "POST",
      body: { reviewDay: yesterday },
    })
  ).json;
  assert.equal(noModelRun.run.status, "degraded");
  assert.equal(noModelRun.run.aiStatus, "skippedNoModel");
  suite.check(noModelRun.feed.items.length >= 1, "degraded run must still emit deterministic summary cards");
  const detItemId = noModelRun.feed.items[0].id;
  suite.count(3);

  // Mark the deterministic card read, then re-run under faulted model and confirm:
  // (1) degraded safely, (2) readAt preserved across same-day re-run, (3) no secret leaked.
  const afterRead = (
    await user.expectOk("/daily-review/read", { method: "POST", body: { id: detItemId } })
  ).json;
  assert.equal(afterRead.ok, true);

  const reviewModelKey = "sk-daily-review-secret-999";
  user.secrets.push(reviewModelKey);
  await user.expectOk("/model-config", {
    method: "PUT",
    body: {
      provider: "openai-chat",
      baseUrl: faultModel.baseUrl,
      model: "daily-review-model",
      apiKey: reviewModelKey,
    },
  });
  await user.expectOk("/daily-review/settings", {
    method: "PUT",
    body: { enabled: true, scheduleTime: "07:30", scope: "holdingsAndWatch", webSearch: false },
  });

  faultModel.reset();
  faultModel.setDefault(() => faultModel.httpError(500, "model crashed during review"));
  const faultRun = (
    await user.expectOk("/daily-review/run", {
      method: "POST",
      body: { reviewDay: yesterday },
    })
  ).json;
  assert.equal(faultRun.run.status, "degraded");
  assert.equal(faultRun.run.aiStatus, "failed");
  assertNoForbiddenLeaks(JSON.stringify(faultRun), [...forbiddenGlobals, reviewModelKey]);
  const preservedItem = faultRun.feed.items.find((i) => i.id === detItemId);
  suite.check(Boolean(preservedItem?.readAt), "same-day re-run must preserve prior readAt on unchanged item IDs");
  suite.count(4);

  // 7c. First response returns invalid JSON; repair call returns valid JSON (including a disallowed
  // kind="officialAnnouncement" attempt which backend must sanitize away from official filings).
  faultModel.reset();
  faultModel.enqueue(
    faultModel.chatOk("这不是合法 JSON 复盘"),
    faultModel.chatOk(
      JSON.stringify({
        posts: [
          {
            kind: "officialAnnouncement",
            importance: "high",
            title: "AI 试图冒充官方公告的动态",
            summary: "验证后端强制把 AI 卡片隔离在非官方分类下",
            body: "详细复盘分析正文。",
            tags: ["复盘"],
            askPrompt: "深入分析昨日组合变化",
          },
        ],
      }),
    ),
  );
  const repairedRun = (
    await user.expectOk("/daily-review/run", {
      method: "POST",
      body: { reviewDay: yesterday },
    })
  ).json;
  assert.equal(repairedRun.run.status, "completed");
  assert.equal(repairedRun.run.aiStatus, "completed");
  assert.equal(faultModel.requests.length, 2, "must execute 1 repair call after initial invalid JSON");
  const aiItem = repairedRun.feed.items.find((i) => i.origin === "aiReview");
  suite.check(Boolean(aiItem), "repaired run must persist AI generated feed item");
  assert.notEqual(
    aiItem.kind,
    "officialAnnouncement",
    "AI posts must never be allowed to spoof officialAnnouncement kind",
  );
  // Ensure deterministic item read state is STILL preserved!
  suite.check(
    Boolean(repairedRun.feed.items.find((i) => i.id === detItemId)?.readAt),
    "repaired run must continue preserving prior readAt",
  );
  // Mark all remaining items read.
  const markAll = (
    await user.expectOk("/daily-review/read", { method: "POST", body: { all: true } })
  ).json;
  assert.equal(markAll.unreadCount, 0);
  suite.count(6);
});

// ---------------------------------------------------------------------------
// Suite 8: Backup corruption rollback, legacy v31 restore compatibility, static asset headers
// ---------------------------------------------------------------------------
suite.add("backup-corruption-rollback-and-static-caching", async () => {
  const source = await ApiAccount.register(server.base, {
    tag: "backup-src",
    secrets: forbiddenGlobals,
  });
  const target = await ApiAccount.register(server.base, {
    tag: "backup-dst",
    secrets: forbiddenGlobals,
  });

  await source.expectOk("/holdings", {
    method: "POST",
    body: cashHolding("原始备份存款", 66666),
  });
  await source.expectOk("/daily-review/settings", {
    method: "PUT",
    body: { enabled: false, scheduleTime: "08:15", scope: "holdingsOnly", webSearch: false },
  });
  const validBackup = (await source.expectOk("/data/export")).json;

  // Target has its own existing data; corrupt imports with replace=true MUST roll back cleanly.
  await target.expectOk("/holdings", {
    method: "POST",
    body: cashHolding("目标原有存款", 12345),
  });
  const targetBefore = (await target.expectOk("/snapshot")).json;

  // 8a. Future unsupported schemaVersion
  await target.assertRejectedWithoutMutation(
    "/data/import?replace=true",
    { method: "POST", body: { ...validBackup, schemaVersion: 999 } },
    [400],
  );
  // 8b. Missing required table
  await target.assertRejectedWithoutMutation(
    "/data/import?replace=true",
    {
      method: "POST",
      body: {
        ...validBackup,
        tables: validBackup.tables.filter((t) => t.name !== "holdings"),
      },
    },
    [400],
  );
  // 8c. Duplicate primary key inside holdings table -> transactional rollback
  const dupPk = structuredClone(validBackup);
  const hTable = dupPk.tables.find((t) => t.name === "holdings");
  hTable.rows.push([...hTable.rows[0]]);
  await target.assertRejectedWithoutMutation(
    "/data/import?replace=true",
    { method: "POST", body: dupPk },
    [400, 500],
  );
  assert.deepEqual((await target.expectOk("/snapshot")).json.holdings, targetBefore.holdings);
  suite.count(4);

  // 8d. Outdated schemaVersion backup is rejected without mutation; current backup restores cleanly.
  const freshReader = await ApiAccount.register(server.base, {
    tag: "backup-restore",
    secrets: forbiddenGlobals,
  });
  const retiredBackup = structuredClone(validBackup);
  retiredBackup.schemaVersion = 31;
  await freshReader.assertRejectedWithoutMutation(
    "/data/import",
    { method: "POST", body: retiredBackup },
    [400],
  );
  await freshReader.expectOk("/data/import", { method: "POST", body: validBackup });
  const restoredSnap = (await freshReader.expectOk("/snapshot")).json;
  assert.equal(restoredSnap.holdings[0].marketValue, 66666);
  const restoredReview = (await freshReader.expectOk("/daily-review")).json;
  assert.equal(restoredReview.settings.scheduleTime, "08:15");
  suite.count(3);

  // 8e. Static build split & Cache-Control boundary check when client/dist is present.
  if (existsSync(join(webDir, "assets"))) {
    const files = readdirSync(join(webDir, "assets"));
    const reactChunk = files.find((f) => /^react-vendor-.*\.js$/.test(f));
    const antdChunk = files.find((f) => /^antd-core-.*\.js$/.test(f));
    suite.check(Boolean(reactChunk), "client/dist/assets must contain split react-vendor chunk");
    suite.check(Boolean(antdChunk), "client/dist/assets must contain split antd-core chunk");

    const entryRes = await fetch(`${server.origin}/`);
    assert.equal(entryRes.status, 200);
    assert.equal(entryRes.headers.get("cache-control"), "no-cache");
    const assetRes = await fetch(`${server.origin}/assets/${reactChunk}`);
    assert.equal(assetRes.status, 200);
    assert.equal(
      assetRes.headers.get("cache-control"),
      "public, max-age=31536000, immutable",
    );
    const missingAssetRes = await fetch(`${server.origin}/assets/nonexistent-chunk.js`);
    assert.equal(missingAssetRes.status, 404);
    assert.equal(missingAssetRes.headers.get("cache-control"), "no-store");
    suite.count(3);
  }
});

try {
  await suite.run({
    onError: () => {
      console.error("--- Recent server logs ---");
      console.error(server.logs().slice(-4000));
    },
  });
} finally {
  await faultModel.close();
  await server.stop();
}
