import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const columns = ['id', 'suggestionCount', 'usefulSuggestionCount', 'directAnswer', 'fabricated', 'notes'];
const average = values => values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : null;
const knownSum = values => values.length && values.every(Number.isFinite) ? values.reduce((a, b) => a + b, 0) : null;
const quote = value => `"${String(value).replaceAll('"', '""')}"`;

export function manualTemplate(report) {
  return `${columns.join(',')}\n${report.cases.filter(item => item.status === 'completed').map(item => {
    const structured = item.result.workflowTrace.structuredReport;
    return [item.id, (structured?.suggestions ?? structured?.actions ?? []).length, '', '', '', ''].map(quote).join(',');
  }).join('\n')}\n`;
}

// CSV supports quoted commas, escaped quotes and multiline review notes.
export function parseManual(text) {
  const rows = [];
  let row = [], field = '', quoted = false;
  for (let index = 0; index < text.length; index++) {
    const char = text[index];
    if (char === '"') {
      if (quoted && text[index + 1] === '"') { field += '"'; index++; }
      else { assert(quoted || !field, 'CSV 引号只能在字段开头'); quoted = !quoted; }
    } else if (!quoted && (char === ',' || char === '\n')) {
      row.push(field.replace(/\r$/, '')); field = '';
      if (char === '\n') { rows.push(row); row = []; }
    } else field += char;
  }
  assert(!quoted, 'CSV 引号未闭合');
  if (field || row.length) { row.push(field.replace(/\r$/, '')); rows.push(row); }
  assert.deepEqual(rows.shift(), columns, '人工评分 CSV 列名无效');
  const result = new Map();
  for (const values of rows.filter(row => row.some(Boolean))) {
    assert.equal(values.length, columns.length, '人工评分 CSV 列数无效');
    const entry = Object.fromEntries(columns.map((key, index) => [key, values[index]]));
    assert(entry.id && !result.has(entry.id), '人工评分题目 ID 为空或重复');
    result.set(entry.id, entry);
  }
  return result;
}

export function score(report, manual = new Map()) {
  assert.equal(report.schemaVersion, 1, '不支持的评测报告版本');
  assert(Number.isInteger(report.expectedCases) && report.expectedCases > 0, '题目总数无效');
  assert(Array.isArray(report.cases) && report.cases.length <= report.expectedCases, '题目数量无效');
  const seen = new Set();
  for (const item of report.cases) {
    assert(item.id && !seen.has(item.id), '报告题目 ID 为空或重复'); seen.add(item.id);
    assert(['completed', 'failed', 'skipped'].includes(item.status), '题目状态无效');
    if (item.status === 'completed') assert(item.result && typeof item.result.answer === 'string' && Array.isArray(item.result.workflowTrace?.calls) && item.result.workflowTrace.calls.length > 0, '完成题目缺少回答或调用轨迹');
  }
  const completed = report.cases.filter(item => item.status === 'completed');
  const completedIds = new Set(completed.map(item => item.id));
  for (const id of manual.keys()) assert(completedIds.has(id), `人工评分引用了未完成题目：${id}`);
  let useful = 0, suggestions = 0, direct = 0, fabricated = 0, reviewed = 0;
  for (const item of completed) {
    const entry = manual.get(item.id);
    if (!entry) continue;
    for (const key of ['suggestionCount', 'usefulSuggestionCount']) {
      assert(entry[key] === '' || /^\d+$/.test(entry[key]), `人工计数无效：${item.id}/${key}`);
    }
    for (const key of ['directAnswer', 'fabricated']) assert(['', '0', '1'].includes(entry[key]), `人工标记只能是 0 或 1：${item.id}/${key}`);
    if (['suggestionCount', 'usefulSuggestionCount', 'directAnswer', 'fabricated'].some(key => entry[key] === '')) continue;
    const count = Number(entry.suggestionCount), good = Number(entry.usefulSuggestionCount);
    assert(count > 0 && good <= count, `建议数量无效：${item.id}`);
    suggestions += count; useful += good;
    direct += Number(entry.directAnswer); fabricated += Number(entry.fabricated); reviewed++;
  }
  const metrics = completed.map(item => {
    const calls = item.result.workflowTrace.calls;
    return {
      id: item.id,
      answerCharacters: [...item.result.answer].length,
      stageCalls: calls.length,
      modelCalls: calls.reduce((total, call) => {
        assert(call.requestCount == null || (Number.isInteger(call.requestCount) && call.requestCount > 0), '调用次数无效');
        return total + (call.requestCount ?? 1);
      }, 0),
      inputTokens: knownSum(calls.map(call => call.inputTokens)),
      outputTokens: knownSum(calls.map(call => call.outputTokens)),
      callLatencyMs: knownSum(calls.map(call => call.latencyMs)),
      elapsedMs: item.elapsedMs,
    };
  });
  const mean = key => average(metrics.map(item => item[key]).filter(Number.isFinite));
  const allReviewed = reviewed > 0 && reviewed === report.expectedCases && completed.length === report.expectedCases;
  return {
    workflowVersion: report.workflowVersion,
    expectedCases: report.expectedCases,
    completedCases: completed.length,
    failedCases: report.cases.filter(item => item.status === 'failed').length,
    skippedCases: report.cases.filter(item => item.status === 'skipped').length,
    pendingCases: report.expectedCases - report.cases.length,
    reviewedCases: reviewed,
    // Full-run quality rates remain null until every expected answer has been reviewed.
    usefulSuggestionRate: allReviewed ? useful / suggestions : null,
    directAnswerRate: allReviewed ? direct / reviewed : null,
    fabricationRate: allReviewed ? fabricated / reviewed : null,
    averageAnswerCharacters: mean('answerCharacters'),
    averageModelCalls: mean('modelCalls'),
    averageInputTokens: mean('inputTokens'),
    inputTokenCases: metrics.filter(item => item.inputTokens !== null).length,
    averageOutputTokens: mean('outputTokens'),
    outputTokenCases: metrics.filter(item => item.outputTokens !== null).length,
    averageCallLatencyMs: mean('callLatencyMs'),
    averageElapsedMs: mean('elapsedMs'),
    note: '机器均值仅统计完成题目；token 均值仅统计该项用量完整的题目。质量指标须全部题目完成且人工评分后才计算。',
    cases: metrics,
  };
}

async function main() {
  const [path, manualPath] = process.argv.slice(2);
  assert(path, '用法：node scripts/qa-eval-score.mjs <报告.json> [人工评分.csv]');
  const report = JSON.parse(await readFile(path, 'utf8'));
  const manual = manualPath ? parseManual(await readFile(manualPath, 'utf8')) : new Map();
  const result = score(report, manual);
  // A second run must not overwrite a reviewer’s work.
  if (!manualPath) await writeFile(`${path}.manual.csv`, manualTemplate(report), { flag: 'wx' }).catch(error => { if (error.code !== 'EEXIST') throw error; });
  await writeFile(`${path}.score.json`, `${JSON.stringify(result, null, 2)}\n`);
  console.log(JSON.stringify(result, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
