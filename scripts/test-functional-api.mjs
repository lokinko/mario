// Real HTTP -> Axum -> SQLite smoke test. Uses disposable data; automatic ECB requests may occur.
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, rm, readFile } from "node:fs/promises";
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
const externalBase = process.env.MARIO_FUNCTIONAL_BASE_URL;
const base = externalBase ?? `http://127.0.0.1:${port}/api`;
let revision;
const token = process.env.MARIO_FUNCTIONAL_TOKEN ?? randomBytes(32).toString("hex");
const masterKey = randomBytes(32).toString("base64");
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
      ...(revision ? {"If-Match":revision} : {}),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.text();
  assert.equal(response.status, status, `${method} ${path}: ${value}`);
  if (response.ok) revision=response.headers.get("x-data-revision") ?? revision;
  count++;
  return value ? (response.headers.get("content-type")?.includes("application/json") ? JSON.parse(value) : value) : undefined;
}
async function startServer() {
  if (externalBase) return;
  const localEnv={...process.env,MARIO_DATA_DIR:dir,MARIO_AUTH_TOKEN:token,MARIO_MASTER_KEY:masterKey};
  delete localEnv.DATABASE_URL;
  child = spawn(
    resolve(process.env.MARIO_TEST_SERVER_BIN ?? (process.platform === "win32" ? "server/target/debug/mario-server.exe" : "server/target/debug/mario-server")),
    ["--port", String(port)],
    {
      env: localEnv,
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
  if (child?.exitCode === null && child.signalCode === null) {
    const exited=once(child, "exit");
    child.kill("SIGTERM");
    await exited;
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
  if (!externalBase) {
    const preflight = await fetch(base + "/health", {
      method: "OPTIONS",
      headers: {
        Origin: "http://localhost:1420",
        "Access-Control-Request-Method": "GET",
        "Access-Control-Request-Headers": "authorization",
      },
    });
    assert.equal(preflight.headers.get("access-control-allow-origin"), "http://localhost:1420");
    for (const path of ["/reminder-settings", "/review-reminders/acknowledge", "/decisions", "/decisions/retired/review", "/rule-effectiveness"])
      assert.equal((await fetch(base + path, { method: "POST", headers: { Authorization: `Bearer ${token}` } })).status, 404);
    await api("/model-config", "PUT", {
      provider: "openai-chat", baseUrl: "https://example.test/v1", model: "test-model", apiKey: "test-encrypted-model-key",
    });
    assert.equal((await api("/model-config")).hasApiKey, true);
    assert(!(await readFile(join(dir, "mario.db"))).includes(Buffer.from("test-encrypted-model-key")));
  }
  let snap = await api("/snapshot");
  await api("/portfolio-checkins", "GET", undefined, 404);
  await api("/portfolio-checkins", "POST", {}, 404);
  for (const [path, method] of [
    ["/goals", "POST"], ["/goals/removed", "PUT"], ["/goals/removed", "DELETE"],
    ["/portfolio-events", "GET"], ["/portfolio-events", "POST"],
    ["/portfolio-events/removed/reverse", "POST"],
    ["/portfolio-events/import/preview", "POST"], ["/portfolio-events/import/commit", "POST"],
  ]) await api(path, method, method === "GET" ? undefined : {}, 404);
  assert.equal(snap.goals, undefined);
  assert.equal(snap.plan.goalProjections, undefined);
  assert.equal(snap.holdings.length, 0);
  const defaults = await api("/analysis-settings");
  assert.equal(defaults.workflow, "deep");
  assert.equal(defaults.contextSelection.includeProfile, true);
  assert.equal(defaults.contextSelection.includeDailyAssets, true);
  assert.equal(defaults.contextSelection.includePortfolioCheckins, undefined);
  assert.equal(defaults.contextSelection.includeGoals, undefined);
  assert.equal(defaults.contextSelection.includePortfolioEvents, undefined);
  const configured = { ...defaults, webSearch: false, useMemory: false, contextSelection: { ...defaults.contextSelection, includeProfile: false } };
  assert.deepEqual(await api("/analysis-settings", "PUT", configured), configured);
  assert.deepEqual(await api("/analysis-settings"), configured);
  await api("/analysis-settings", "PUT", { ...configured, workflow: "invalid" }, 400);
  assert.deepEqual(await api("/analysis-settings"), configured);
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
  await api("/fx-settings", "PUT", { currency: "USD", rate: 7 });
  const foreign = await api("/holdings", "POST", holding("美元账户", 100, "USD"));
  assert.equal(foreign.valuationStatus.comparable, true);
  const fxHolding = foreign.holdings.find((h) => h.currency === "USD");
  assert.equal(fxHolding.fxRateToBase, 7);
  assert.equal(fxHolding.fxRateSource, "central_manual");
  assert.equal((await api("/daily-assets")).records[0].totalAssets, 100700);
  await api(`/holdings/${fxHolding.id}`, "DELETE");
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
    "PASS assets: unified FX, zero, version conflict, idempotency, daily history",
  );

  await api(`/holdings/${cash}`, "PUT", {
    ...holding("现金", 102100),
    valuationDate: today,
  });

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
  const originalFile = Buffer.from("黄金储备：原始重要信息\n\n下一段", "utf8");
  const fileSource = await api("/research-evidence", "POST", {
    content: "", attachments: [{ name: "资料.txt", dataBase64: originalFile.toString("base64") }],
  });
  assert.equal(fileSource.claim, "");
  assert.equal(fileSource.attachments.length, 1);
  const attachment = fileSource.attachments[0];
  assert.equal(attachment.byteSize, originalFile.length);
  assert.match(attachment.extractionNote, /已提取文字/);
  assert.equal("dataBase64" in attachment, false);
  const downloadPath = `/research-evidence/${fileSource.id}/attachments/${attachment.id}`;
  assert.equal((await fetch(base + downloadPath)).status, 401);
  const download = await api(downloadPath);
  assert.equal(download.name, "资料.txt");
  assert.deepEqual(Buffer.from(download.dataBase64, "base64"), originalFile);
  await api(`/research-evidence/${source.id}/attachments/${attachment.id}`, "GET", undefined, 404);
  const filePreview = await api("/analysis/preview", "POST", {
    question: "黄金储备", workflow: "quick", useMemory: false, reflect: false,
    exploreAlternatives: false, webSearch: false,
  });
  assert(filePreview.evidenceCandidates.some(item => item.id === fileSource.id && item.claim.includes("原始重要信息")));
  assert(!JSON.stringify(filePreview).includes(originalFile.toString("base64")));
  await api("/research-evidence", "POST", {content: "", attachments: [{name: "bad.txt", dataBase64: "bad!"}]}, 400);
  await api(`/research-evidence/${fileSource.id}/status`, "PUT", { active: false });
  assert.deepEqual(Buffer.from((await api(downloadPath)).dataBase64, "base64"), originalFile);
  const largeFile = Buffer.alloc(2 * 1024 * 1024, 65);
  const largeSource = await api("/research-evidence", "POST", {
    content: "", attachments: [{ name: "large.txt", dataBase64: largeFile.toString("base64") }],
  });
  assert.equal(largeSource.attachments[0].byteSize, largeFile.length);
  assert.match(largeSource.attachments[0].extractionNote, /前 20000/);
  console.log("PASS evidence: text, file upload/download, attachment retrieval, authentication, archive, body limit");

  const ruleInput = {
    category: "风险",
    statement: "检查应急资金",
    trigger: "投资前",
    rationale: "保留流动性",
    active: true,
  };
  const rule = await api("/investment-rules", "POST", ruleInput);
  await api("/decisions", "GET", undefined, 404);
  await api("/decisions/retired/review", "PUT", {}, 404);
  await api("/rule-effectiveness", "GET", undefined, 404);
  assert.equal("dueDecisionCount" in (await api("/review-reminders")), false);
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
  assert.equal("decisionTotal" in periodic.snapshot, false);
  assert.equal("reviewedDecisions" in periodic.snapshot, false);
  const reviewContent = "我的观点：继续观察。\n\n顾虑：资料不足。";
  await api("/system-reviews", "POST", { content: " \n\t " }, 400);
  const textReview = await api("/system-reviews", "POST", { content: reviewContent });
  assert.equal(textReview.content, reviewContent);
  assert.equal(textReview.adherenceScore, null);
  assert.equal(textReview.nextReviewDate, "");
  assert.equal((await api("/system-reviews")).find((item) => item.id === textReview.id).content, reviewContent);
  assert.equal((await api("/review-reminders")).periodicReviewDue, false);
  console.log("PASS text reviews: content-only save, full original text, no invented scores or due dates");

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
      includeDailyAssets: false,
    },
  });
  assert.equal(omitted.payload.dailyAssetHistory, undefined);
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
  if (!externalBase) {
    assert.equal((await api("/model-config")).hasApiKey, true);
    await api("/model-key", "DELETE");
    assert.equal((await api("/model-config")).hasApiKey, false);
  }
  assert.deepEqual(await api("/analysis-settings"), configured);
  snap=await api("/snapshot");
  assert.equal(snap.holdings[0].marketValue, 102200);
  assert.equal((await api("/daily-assets")).records[0].totalAssets, 102200);
  assert.equal((await api("/system-reviews")).find((item) => item.id === textReview.id).content, reviewContent);
  const execution = {
    expectedCashBaseline: snap.cashBalances.find(c=>c.currency==="CNY").baselineId,
    requestId: randomUUID(), holdingId: "", expectedRevision: null,
    newHolding: { name: "交易编辑测试", symbol: "", assetClass: "股票", currency: "CNY" },
    kind: "buy", occurredOn: today, sequence: 1, quantity: "10", price: "10", grossAmount: "100", fees: "2", taxes: "1",
    account: "测试券商", externalId: "trade-edit-1", note: "", settlementMode: "settle",
  };
  snap = await api("/holding-trades", "POST", execution);
  const trade = snap.holdingTrades.find(t => t.id === execution.requestId);
  assert.equal(snap.holdingTrades.length, 1);
  assert.equal(snap.holdings.find(h => h.id === cash).marketValue, 102097);
  assert.equal(snap.totalValue, 102197);
  assert.equal(snap.holdings.find(h => h.id === trade.holdingId).currentPrice, 10);
  const correction = {
    expectedRevision: snap.holdingRevisions[trade.holdingId],
    kind: "buy", occurredOn: today, sequence: 1, quantity: "12", price: "11", grossAmount: "132", fees: "3", taxes: "1",
    account: "测试券商", externalId: "trade-edit-corrected", note: "修正录入",
  };
  snap = await api(`/holding-trades/${trade.id}`, "PUT", correction);
  assert.equal(snap.holdingTrades.length, 1);
  assert.equal(snap.holdingTrades.find(t => t.id === trade.id).createdAt, trade.createdAt);
  assert.equal(snap.holdings.find(h => h.id === trade.holdingId).costBasis, 136);
  assert.equal(snap.holdings.find(h => h.id === cash).marketValue, 102064);
  const correctedRevision = snap.holdingRevisions[trade.holdingId];
  snap = await api(`/holding-trades/${trade.id}`, "PUT", correction);
  assert.equal(snap.holdingRevisions[trade.holdingId], correctedRevision);
  assert.equal(snap.holdings.find(h => h.id === cash).marketValue, 102064);
  await api(`/holding-trades/${trade.id}`, "PUT", { ...correction, note: "过期修改" }, 409);
  await api(`/holding-trades/${trade.id}`, "PUT", { ...correction, expectedRevision: correctedRevision, kind: "sell" }, 400);
  await api("/holding-trades", "POST", { ...execution, requestId: randomUUID(), kind: "opening" }, 400);
  // The old fields are rejected as a format error, never silently ignored.
  await api("/holding-trades", "POST", { ...execution, requestId: randomUUID(), voidOf: trade.id }, 422);
  assert.deepEqual((await api("/snapshot")).holdingTrades, snap.holdingTrades);
  const dividend = {
    ...execution, requestId: randomUUID(), holdingId: trade.holdingId, newHolding: null,
    expectedRevision: correctedRevision, kind: "dividend", occurredOn: today, sequence: 2,
    quantity: "0", price: "0", grossAmount: "20", fees: "1", taxes: "3", externalId: "dividend-1",
  };
  snap = await api("/holding-trades", "POST", dividend);
  assert.equal(snap.holdingTradeSummaries[trade.holdingId].dividendIncome, "16");
  assert.equal(snap.holdings.find(h => h.id === cash).marketValue, 102080);
  assert.equal(snap.holdingTradeSummaries[trade.holdingId].costBasis, "136");
  assert.equal(snap.holdingTradeSummaries[trade.holdingId].quantity, "12");
  const sale = {
    ...dividend, requestId: randomUUID(), expectedRevision: snap.holdingRevisions[trade.holdingId],
    kind: "sell", sequence: 3, quantity: "12", price: "20", grossAmount: "240", fees: "1", taxes: "2", externalId: "sale-1",
  };
  snap = await api("/holding-trades", "POST", sale);
  assert.equal(snap.holdingTradeSummaries[trade.holdingId].settledProfit, "117");
  assert.equal(snap.holdings.find(h => h.id === trade.holdingId).quantity, 0);
  assert.equal(snap.holdings.find(h => h.id === cash).marketValue, 102317);
  const records = snap.holdingTrades;
  await stopServer(); await startServer();
  assert.deepEqual((await api("/snapshot")).holdingTrades, records);
  const pastRecords = (await api("/daily-assets")).records.filter(r => r.day < today);
  const deleteInput = () => ({ expectedRevision: snap.holdingRevisions[trade.holdingId] });
  await api(`/holding-trades/${sale.requestId}`, "DELETE", { expectedRevision: correctedRevision }, 409);
  await api(`/holding-trades/${sale.requestId}`, "DELETE", {}, 422);
  await api(`/holding-trades/${trade.id}`, "DELETE", deleteInput(), 400);
  assert.deepEqual((await api("/snapshot")).holdingTrades, records);
  snap = await api(`/holding-trades/${sale.requestId}`, "DELETE", deleteInput());
  assert.equal(snap.holdings.find(h => h.id === trade.holdingId).quantity, 12);
  assert.equal(snap.holdingTradeSummaries[trade.holdingId].settledProfit, "16");
  assert.equal(snap.holdingTradeSummaries[trade.holdingId].costBasis, "136");
  assert(!snap.holdingTradeSummaries[trade.holdingId].profits[sale.requestId]);
  await api(`/holding-trades/${trade.id}`, "DELETE", deleteInput(), 400);
  snap = await api(`/holding-trades/${dividend.requestId}`, "DELETE", deleteInput());
  assert.equal(snap.holdingTradeSummaries[trade.holdingId].settledProfit, "0");
  const lastDelete = deleteInput();
  snap = await api(`/holding-trades/${trade.id}`, "DELETE", lastDelete);
  assert.equal(snap.holdingTrades.length, 0);
  assert.equal(snap.holdingTradeSummaries[trade.holdingId], undefined);
  const emptySecurity = snap.holdings.find(h => h.id === trade.holdingId);
  assert.equal(emptySecurity.quantity, 0);
  assert.equal(emptySecurity.costBasis, 0);
  assert.equal(emptySecurity.marketValue, 0);
  assert.equal(snap.holdings.find(h => h.id === cash).marketValue, 102200);
  const deletedRevision = snap.holdingRevisions[trade.holdingId];
  snap = await api(`/holding-trades/${trade.id}`, "DELETE", lastDelete);
  assert.equal(snap.holdingRevisions[trade.holdingId], deletedRevision);
  assert.deepEqual((await api("/daily-assets")).records.filter(r => r.day < today), pastRecords);
  await stopServer(); await startServer();
  snap = await api("/snapshot");
  assert.equal(snap.holdingTrades.length, 0);
  assert.equal(snap.holdings.find(h => h.id === trade.holdingId).quantity, 0);
  console.log("PASS real trading: exact amounts, edits, confirmed deletion API, atomic rejection, dividends, restart, linked cash settlement and unchanged past observations");
  // Exercise browser JSON -> HTTP JSON parser with a recurring weighted average.
  const oppoBuy = {
    ...execution, requestId: randomUUID(),
    newHolding: { name: "OPPO 股票", symbol: "", assetClass: "股票", currency: "CNY" },
    account: "内部股票账户", externalId: "oppo-1", taxes: "0",
  };
  snap = await api("/holding-trades", "POST", oppoBuy);
  const oppoId = snap.holdingTrades.find(t => t.id === oppoBuy.requestId).holdingId;
  snap = await api("/holding-trades", "POST", {
    ...oppoBuy, requestId: randomUUID(), holdingId: oppoId, newHolding: null,
    expectedRevision: snap.holdingRevisions[oppoId], sequence: 2,
    quantity: "7", price: "17.13", grossAmount: "119.91", fees: "0", externalId: "oppo-2",
  });
  const originalOPPO = snap.holdings.find(h => h.id === oppoId);
  const oppoTrades = snap.holdingTrades;
  const oppoSummary = snap.holdingTradeSummaries[oppoId];
  for (const currentPrice of [25, 30.12345678]) {
    const { id, ...input } = snap.holdings.find(h => h.id === oppoId);
    snap = await api(`/holdings/${id}`, "PUT", {
      ...input, currentPrice, marketValue: input.quantity * currentPrice,
    });
    const updated = snap.holdings.find(h => h.id === id);
    assert.equal(updated.currentPrice, currentPrice);
    assert.equal(updated.marketValue, 17 * currentPrice);
    for (const field of ["quantity", "costPrice", "costBasis"]) {
      assert.equal(updated[field], originalOPPO[field]);
    }
    assert.deepEqual(snap.holdingTrades, oppoTrades);
    assert.deepEqual(snap.holdingTradeSummaries[id], oppoSummary);
  }
  const { id: oppoHoldingId, ...oppoInput } = snap.holdings.find(h => h.id === oppoId);
  for (const changes of [{ quantity: 18 }, { costPrice: 14 }, { costBasis: 238 }, { currency: "USD" }, { assetClass: "基金" }]) {
    await api(`/holdings/${oppoHoldingId}`, "PUT", { ...oppoInput, ...changes }, 400);
  }
  await stopServer(); await startServer();
  snap = await api("/snapshot");
  const reopenedOPPO = snap.holdings.find(h => h.id === oppoId);
  assert.equal(reopenedOPPO.currentPrice, 30.12345678);
  assert.equal(reopenedOPPO.costBasis, originalOPPO.costBasis);
  assert.deepEqual(snap.holdingTrades, oppoTrades);
  assert.deepEqual(snap.holdingTradeSummaries[oppoId], oppoSummary);
  assert.equal(snap.holdings.find(h => h.id === cash).marketValue, 101978.09);
  console.log("PASS internal OPPO stock: manual price edits, JSON average-cost round trip, protected execution totals and linked cash persistence");
  const noFunds = { ...execution, expectedCashBaseline:"", requestId: randomUUID(), newHolding: { ...execution.newHolding, currency: "USD" } };
  const priorHoldings = snap.holdings;
  const rejected = await api("/holding-trades", "POST", noFunds, 400);
  assert(JSON.stringify(rejected).includes("USD 存款余额不足"));
  snap = await api("/snapshot");
  assert.deepEqual(snap.holdings, priorHoldings);
  await api("/holdings", "POST", holding("重复人民币存款", 100), 409);
  await api("/holdings", "POST", holding("欧元存款", 100, "EUR"), 400);
  console.log(
    `PASS restart persistence; ${count} authenticated HTTP assertions total`,
  );
  if (!externalBase) {
    await stopServer();
    const exportPath=join(dir,'migration.json');
    const binary=resolve('server/target/debug/mario-server'+(process.platform==='win32'?'.exe':''));
    const exportResult=spawnSync(binary,['--export-data',exportPath],{env:{...process.env,MARIO_DATA_DIR:dir},encoding:'utf8'});
    assert.equal(exportResult.status,0,exportResult.stderr);
    const exported=JSON.parse(await readFile(exportPath,'utf8'));
    assert(exported.tables.find(t=>t.name==='holdings').rows.length>0);
    const fxSettings=exported.tables.find(t=>t.name==='settings');
    assert(fxSettings.rows.length>0);
    assert(fxSettings.rows.every(row=>row[0].Text?.startsWith('fx.')));
    const overwrite=spawnSync(binary,['--export-data',exportPath],{env:{...process.env,MARIO_DATA_DIR:dir},encoding:'utf8'});
    assert.notEqual(overwrite.status,0);
    console.log('PASS legacy SQLite CLI export and overwrite protection');
  }
} finally {
  await stopServer();
  await rm(dir, { recursive: true, force: true });
}
