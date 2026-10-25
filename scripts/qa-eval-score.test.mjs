import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { manualTemplate, parseManual, score } from './qa-eval-score.mjs';

function fixture() {
  return { schemaVersion: 1, workflowVersion: 'v7', expectedCases: 2, cases: ['a', 'b'].map((id, index) => ({
    id, status: 'completed', elapsedMs: 300,
    result: { answer: '回答😀', workflowTrace: { structuredReport: { actions: [{}, {}] }, calls: [
      { requestCount: index ? 2 : null, inputTokens: index ? null : 100, outputTokens: 20, latencyMs: 80 },
      { requestCount: 1, inputTokens: 50, outputTokens: 10, latencyMs: 120 },
    ] } },
  })) };
}

test('按真实请求次数计数，Unicode 按字符计数，缺少 token 不当成零', () => {
  const result = score(fixture());
  assert.equal(result.averageModelCalls, 2.5);
  assert.equal(result.averageAnswerCharacters, 3);
  assert.equal(result.averageInputTokens, 150);
  assert.equal(result.inputTokenCases, 1);
  assert.equal(result.averageCallLatencyMs, 200);
  assert.equal(result.fabricationRate, null);
  assert.equal(result.directAnswerRate, null);
});

test('失败、跳过或中断的评测不会变成已通过的质量指标', () => {
  const report = fixture();
  report.cases[1] = { id: 'b', status: 'failed', error: '模型不可用' };
  report.expectedCases = 4;
  report.cases.push({ id: 'c', status: 'skipped' });
  const manual = parseManual('id,suggestionCount,usefulSuggestionCount,directAnswer,fabricated,notes\na,2,2,1,0,\n');
  const result = score(report, manual);
  assert.equal(result.completedCases, 1);
  assert.equal(result.failedCases, 1);
  assert.equal(result.skippedCases, 1);
  assert.equal(result.pendingCases, 1);
  assert.equal(result.reviewedSubset.cases, 1);
  assert.equal(result.reviewedSubset.usefulSuggestionRate, 1);
  assert.equal(result.usefulSuggestionRate, null);
  assert.equal(result.fabricationRate, null);
});

test('人工评分按建议数加权，并支持逗号、引号和换行笔记', () => {
  const manual = parseManual('id,suggestionCount,usefulSuggestionCount,directAnswer,fabricated,notes\na,3,2,1,0,"有对象,但缺前提"\nb,1,1,0,1,"引用""错误""\n待修复"\n');
  const result = score(fixture(), manual);
  assert.equal(result.usefulSuggestionRate, 0.75);
  assert.equal(result.directAnswerRate, 0.5);
  assert.equal(result.fabricationRate, 0.5);
  assert.equal(manual.get('b').notes, '引用"错误"\n待修复');
});

test('空白人工模板保留未知指标，非法或错配评分被拒绝', () => {
  const report = fixture();
  assert.equal(score(report, parseManual(manualTemplate(report))).reviewedCases, 0);
  for (const row of ['a,1,2,1,0,', 'a,2,-1,1,0,', 'a,2,1,yes,0,', 'unknown,2,1,1,0,', 'a,2,1,1,0,\na,2,1,1,0,']) {
    assert.throws(() => score(report, parseManual(`id,suggestionCount,usefulSuggestionCount,directAnswer,fabricated,notes\n${row}\n`)));
  }
});

test('命令行生成并读取报告旁的人工评分，重复运行保留评阅内容', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'mario-qa-score-'));
  try {
    const path = join(directory, 'report.json');
    const csvPath = `${path}.manual.csv`;
    await writeFile(path, JSON.stringify(fixture()));
    const run = (...args) => spawnSync(process.execPath, [new URL('./qa-eval-score.mjs', import.meta.url).pathname, path, ...args], { encoding: 'utf8' });
    assert.equal(run().status, 0);
    assert.equal(JSON.parse(await readFile(`${path}.score.json`, 'utf8')).reviewedCases, 0);
    const reviewed = 'id,suggestionCount,usefulSuggestionCount,directAnswer,fabricated,notes\na,2,2,1,0,已复核\nb,2,1,1,0,已复核\n';
    await writeFile(csvPath, reviewed);
    assert.equal(run().status, 0);
    assert.equal(await readFile(csvPath, 'utf8'), reviewed);
    const scored = run(csvPath);
    assert.equal(scored.status, 0, scored.stderr);
    assert.equal(JSON.parse(scored.stdout).usefulSuggestionRate, 0.75);
    assert.equal(JSON.parse(await readFile(`${path}.score.json`, 'utf8')).fabricationRate, 0);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
