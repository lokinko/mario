// Real HTTP -> Axum -> SQLite smoke test. Uses only disposable data and no provider calls.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { randomBytes, randomUUID } from "node:crypto";
import { setTimeout as sleep } from "node:timers/promises";

const dir = await mkdtemp(join(tmpdir(), "mario-functional-"));
const socket = createServer();
socket.listen(0, "127.0.0.1");
await once(socket, "listening");
const port = socket.address().port;
await new Promise((r) => socket.close(r));
const base = `http://127.0.0.1:${port}/api`;
const token = randomBytes(32).toString("hex");
let child,
  log = "",
  count = 0;
const date = (n) =>
  new Date(Date.now() + n * 86400000).toISOString().slice(0, 10);
const today = date(0),
  start = date(-10),
  middle = date(-5);
async function api(path, method = "GET", body, status = 200) {
  const response = await fetch(base + path, {
    method,
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.text();
  assert.equal(response.status, status, `${method} ${path}: ${value}`);
  count++;
  return value ? JSON.parse(value) : undefined;
}
async function startServer() {
  child = spawn(
    resolve("server/target/debug/mario-server"),
    ["--port", String(port)],
    {
      env: { ...process.env, MARIO_DATA_DIR: dir, MARIO_AUTH_TOKEN: token },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  child.stdout.on("data", (b) => (log += b));
  child.stderr.on("data", (b) => (log += b));
  for (let i = 0; i < 100; i++) {
    try {
      const r = await fetch(base + "/health", {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (r.ok) return;
    } catch {}
    if (child.exitCode !== null) throw Error(log);
    await sleep(50);
  }
  throw Error(`Server failed to become ready: ${log}`);
}
async function stopServer() {
  if (child?.exitCode === null) {
    child.kill("SIGTERM");
    await once(child, "exit");
  }
}
const holding = (name, amount, currency = "CNY") => ({
  symbol: "",
  name,
  assetClass: "现金",
  marketValue: amount,
  costBasis: 0,
  targetPct: 0,
  currency,
  fxRateToBase: null,
  valuationDate: start,
  fxRateSource: "",
  fxRateObservedOn: "",
});
try {
  await startServer();
  assert.equal((await fetch(base + "/daily-assets")).status, 401);
  let snap = await api("/snapshot");
  assert.equal(snap.holdings.length, 0);
  const profile = {
    ...snap.profile,
    monthlyIncome: 18000,
    monthlyExpense: 8000,
    emergencyFund: 60000,
    liabilities: 10000,
  };
  await api("/profile", "PUT", profile);
  snap = await api("/holdings", "POST", holding("现金", 100000));
  const cash = snap.holdings[0].id;
  let unknown = await api("/holdings", "POST", holding("美元账户", 100, "USD"));
  assert.equal(unknown.valuationStatus.comparable, false);
  const fxId = unknown.holdings.find((h) => h.currency === "USD").id;
  assert.equal((await api("/daily-assets")).records[0].totalAssets, null);
  await api(`/holdings/${fxId}`, "DELETE");
  await api("/daily-assets/ensure", "POST", { timezone: "UTC" });
  const day = (await api("/daily-assets")).today;
  snap = await api("/snapshot");
  const amount = {
    amount: 0,
    expectedRevision: snap.holdingRevisions[cash],
    requestId: randomUUID(),
  };
  snap = await api(`/holdings/${cash}/amount`, "PUT", amount);
  assert.equal(snap.holdings[0].marketValue, 0);
  await api(`/holdings/${cash}/amount`, "PUT", amount);
  await api(
    `/holdings/${cash}/amount`,
    "PUT",
    { ...amount, amount: 10, requestId: randomUUID() },
    409,
  );
  snap = await api(`/holdings/${cash}`, "PUT", holding("现金", 100000));
  const history = await api("/daily-assets");
  assert.equal(history.records.length, 1);
  assert.equal(history.records[0].netAssets, 90000);
  const same = await api(`/daily-assets/compare?from=${day}&to=${day}`);
  assert.equal(same.amountChange, 0);
  await api("/daily-assets?from=bad", "GET", undefined, 400);
  console.log(
    "PASS assets: FX incompleteness, zero, version conflict, idempotency, daily history",
  );

  const goal = {
    name: "备用金目标",
    targetAmount: 200000,
    currentAmount: 10000,
    monthlyContribution: 3000,
    targetDate: date(730),
    priority: "高",
  };
  snap = await api("/goals", "POST", goal);
  const goalId = snap.goals[0].id;
  assert.equal(snap.plan.goalProjections.length, 1);
  snap = await api(`/goals/${goalId}`, "PUT", {
    ...goal,
    currentAmount: 200000,
  });
  assert.equal(snap.plan.goalProjections[0].status, "reached");
  await api(`/goals/${goalId}`, "DELETE");
  console.log("PASS goals: create, edit, deterministic projection, delete");

  const checkin = {
    periodLabel: "基线",
    externalCashFlow: 0,
    note: "HTTP 功能测试",
    resetBaseline: false,
    useLedgerCashFlows: true,
  };
  await api("/portfolio-checkins", "POST", checkin);
  const flow = {
    eventType: "deposit",
    source: "test",
    externalId: "deposit-1",
    assetName: "",
    amount: 1000,
    currency: "CNY",
    fxRateToBase: null,
    occurredOn: middle,
    note: "新增投入",
  };
  const deposit = await api("/portfolio-events", "POST", flow);
  await api("/portfolio-events", "POST", flow, 409);
  const reversal = await api(
    `/portfolio-events/${deposit.id}/reverse`,
    "POST",
    { occurredOn: middle, note: "测试冲正" },
  );
  assert.equal(reversal.amount, -1000);
  await api(
    `/portfolio-events/${deposit.id}/reverse`,
    "POST",
    { occurredOn: middle, note: "重复" },
    409,
  );
  const csvText = `source,external_id,event_type,occurred_on,amount,currency,asset_name,note\ntest,csv-1,deposit,${middle},2000,CNY,,CSV入金`;
  let preview = await api("/portfolio-events/import/preview", "POST", {
    csvText,
  });
  assert.equal(preview.readyCount, 1);
  const imported = await api("/portfolio-events/import/commit", "POST", {
    csvText,
    previewRevision: preview.previewRevision,
  });
  assert.equal(imported.insertedCount, 1);
  preview = await api("/portfolio-events/import/preview", "POST", { csvText });
  assert.equal(preview.duplicateCount, 1);
  const altered = await api("/portfolio-events/import/preview", "POST", {
    csvText: csvText.replace("2000", "9999"),
  });
  assert.equal(altered.errorCount, 1);
  await api(`/holdings/${cash}`, "PUT", {
    ...holding("现金", 102100),
    valuationDate: today,
  });
  const final = await api("/portfolio-checkins", "POST", {
    ...checkin,
    periodLabel: "期末",
  });
  assert.equal(final.totalChange, 2100);
  assert.equal(final.externalCashFlow, 2000);
  assert.equal(final.valuationResidual, 100);
  await api(
    "/portfolio-events",
    "POST",
    { ...flow, externalId: "frozen" },
    400,
  );
  console.log(
    "PASS ledger: baseline, event deduplication, reversal, CSV import, attribution, frozen period",
  );

  const evidence = {
    assetName: "现金",
    title: "利率资料",
    publisher: "测试来源",
    sourceUrl: "https://example.com/rate",
    sourceTier: "一手来源",
    evidenceType: "数据发布",
    stance: "支持",
    asOfDate: start,
    claim: "用于功能测试的资料，不构成真实行情",
    notes: "测试",
  };
  const source = await api("/research-evidence", "POST", evidence);
  await api(
    "/research-evidence",
    "POST",
    { ...evidence, sourceUrl: "http://example.com" },
    400,
  );
  await api(`/research-evidence/${source.id}/status`, "PUT", { active: false });
  assert.equal(
    (await api("/research-evidence")).find((e) => e.id === source.id).active,
    false,
  );
  console.log("PASS evidence: save, unsafe source rejection, archive");

  const ruleInput = {
    category: "风险",
    statement: "检查应急资金",
    trigger: "投资前",
    rationale: "保留流动性",
    active: true,
  };
  const rule = await api("/investment-rules", "POST", ruleInput);
  const decision = {
    id: randomUUID(),
    assetName: "现金",
    thesis: "保持流动性",
    counterThesis: "机会成本",
    expectedReturnPct: 2,
    downsidePct: 1,
    confidencePct: 70,
    positionPct: 20,
    invalidation: "支出增加",
    reviewDate: start,
    ruleChecks: [
      {
        ruleId: rule.id,
        ruleRevision: rule.revision,
        category: rule.category,
        statement: rule.statement,
        trigger: rule.trigger,
        status: "遵守",
        note: "",
      },
    ],
  };
  await api("/decisions", "POST", { ...decision, ruleChecks: [] }, 400);
  await api("/decisions", "POST", decision, 204);
  await api("/reminder-settings", "PUT", { enabled: true });
  let reminder = await api("/review-reminders");
  assert.equal(reminder.dueDecisionCount, 1);
  await api("/review-reminders/acknowledge", "POST", {
    fingerprint: reminder.fingerprint,
  });
  assert.equal((await api("/review-reminders")).shouldNotify, false);
  await api(
    `/decisions/${decision.id}/review`,
    "PUT",
    {
      outcomeSummary: "保留了流动性",
      actualReturnPct: null,
      thesisStatus: "成立",
      processRating: 4,
      lessons: "继续检查预算",
    },
    204,
  );
  const recorded = (await api("/decisions"))[0];
  assert.equal(recorded.thesis, decision.thesis);
  assert.equal(recorded.review.processRating, 4);
  await api(`/investment-rules/${rule.id}`, "PUT", {
    ...ruleInput,
    statement: "每月检查应急资金",
  });
  assert.equal((await api(`/investment-rules/${rule.id}/history`)).length, 2);
  const periodic = await api("/system-reviews", "POST", {
    periodLabel: "测试周期",
    adherenceScore: 4,
    processSummary: "检查预算",
    ruleViolations: "无",
    lessons: "保持记录",
    nextActions: "持续跟踪",
    nextReviewDate: date(30),
  });
  assert.equal(periodic.snapshot.portfolioValue, 102100);
  const memories = await api("/memories");
  assert.ok(memories.length > 0);
  const memory = await api(
    `/memories/${encodeURIComponent(memories[0].id)}/preference`,
    "PUT",
    { preference: "hidden", note: "测试屏蔽" },
  );
  assert.equal(memory.preference, "hidden");
  assert.equal((await api("/rule-effectiveness")).reviewedChecks, 1);
  console.log(
    "PASS decisions/reviews/rules/memory/reminders: workflow and immutable original judgment",
  );

  const request = {
    question: "分析我的现金资产变化",
    workflow: "quick",
    useMemory: false,
    reflect: false,
    exploreAlternatives: false,
    webSearch: false,
    dailyAssetRange: { from: day, to: day },
  };
  const analysis = await api("/analysis/preview", "POST", request);
  assert.ok(analysis.payload.dailyAssetHistory);
  const redacted = await api("/analysis/preview", "POST", {
    ...request,
    contextSelection: { includeProfile: false },
  });
  assert.ok(
    !JSON.stringify(redacted.payload.dailyAssetHistory).includes(
      '"liabilities"',
    ),
  );
  const omitted = await api("/analysis/preview", "POST", {
    ...request,
    contextSelection: {
      includePortfolioCheckins: false,
      includePortfolioEvents: false,
    },
  });
  assert.equal(omitted.payload.dailyAssetHistory, undefined);
  assert.equal(omitted.payload.portfolioEvents, undefined);
  await api(`/holdings/${cash}`, "PUT", {
    ...holding("现金", 102200),
    valuationDate: today,
  });
  await api(
    "/analysis",
    "POST",
    { ...request, previewRevision: analysis.contextRevision },
    400,
  );
  assert.deepEqual(await api("/analyses"), []);
  console.log(
    "PASS AI boundary: preview, double authorization, stale preview rejection; no model call",
  );

  await stopServer();
  await startServer();
  assert.equal((await api("/snapshot")).holdings[0].marketValue, 102200);
  assert.equal((await api("/daily-assets")).records[0].totalAssets, 102200);
  assert.equal((await api("/decisions"))[0].review.processRating, 4);
  console.log(
    `PASS restart persistence; ${count} authenticated HTTP assertions total`,
  );
} finally {
  await stopServer();
  await rm(dir, { recursive: true, force: true });
}
