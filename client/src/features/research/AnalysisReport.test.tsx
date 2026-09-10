// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { AdviceCards, SourceEvidence } from "./AnalysisReport";
import type {
  AnalysisEvidenceReference,
  StructuredAnalysis,
} from "../../types";
afterEach(cleanup);
const evidence: AnalysisEvidenceReference[] = [
  {
    id: "e1",
    title: "基金年度报告",
    publisher: "基金公司",
    sourceUrl: "https://example.com/report",
    sourceTier: "一手来源",
    asOfDate: "2025-12-31",
    claim: "前十大持仓合计占比 70%",
    notes: "历史期末值",
    stance: "反驳",
  },
];
const report: StructuredAnalysis = {
  verdict: "先检查集中度",
  facts: [
    { statement: "你持有这只基金", basis: "user_data", evidenceIds: [] },
    {
      statement: "基金自身也存在集中度",
      basis: "research_evidence",
      evidenceIds: ["e1"],
    },
  ],
  inferences: [],
  unknowns: ["最新仓位"],
  options: [],
  reviewTriggers: ["下次披露"],
  actions: [
    {
      action: "核对穿透后的集中度",
      rationale: "个人仓位与基金内部集中度需要一起评估",
      supportingFactIndices: [0, 1],
      evidenceLimits: ["年报不能代表今天的仓位"],
      reversible: true,
      reviewTrigger: "新报告公布",
    },
    {
      action: "补充用钱时间",
      rationale: "确认期限",
      supportingFactIndices: [0],
      evidenceLimits: ["尚未了解你的资金需求"],
      reversible: true,
      reviewTrigger: "目标变化",
    },
  ],
};
it("shows advice-specific facts, sources, caveats and carries them into the confirmation draft", () => {
  const onCreateDecisionDraft = vi.fn();
  render(
    <AdviceCards
      report={report}
      evidence={evidence}
      analysisId="a1"
      grounding={[
        {
          actionIndex: 0,
          status: "supported",
          reason: "本次资料支持核对集中度",
          checks: [],
        },
      ]}
      onCreateDecisionDraft={onCreateDecisionDraft}
    />,
  );
  expect(
    screen.getByRole("link", { name: "基金年度报告 ↗" }).getAttribute("href"),
  ).toBe("https://example.com/report");
  expect(screen.getByText("年报不能代表今天的仓位")).toBeTruthy();
  expect(
    screen.getByText(
      "未关联外部来源；这条建议不能据此说明当前市场或产品状况。",
    ),
  ).toBeTruthy();
  fireEvent.click(screen.getAllByRole("button", { name: "确认行动草稿" })[0]);
  expect(onCreateDecisionDraft.mock.calls[0][0]).toMatchObject({
    sourceAnalysisId: "a1",
    sourceActionIndex: 0,
  });
  expect(onCreateDecisionDraft.mock.calls[0][0].thesis).toContain(
    "基金自身也存在集中度",
  );
  expect(onCreateDecisionDraft.mock.calls[0][0].thesis).toContain(
    "年报不能代表今天的仓位",
  );
});
it("labels older advice as missing provenance instead of assigning unrelated facts", () => {
  render(
    <AdviceCards
      report={{
        ...report,
        actions: [
          {
            action: "旧建议",
            rationale: "旧原因",
            reversible: true,
            reviewTrigger: "以后",
          },
        ],
      }}
      evidence={evidence}
      analysisId="old"
      onCreateDecisionDraft={vi.fn()}
    />,
  );
  expect(screen.getByText(/旧回答没有保存逐条建议的证据关联/)).toBeTruthy();
  expect(screen.queryByText("基金年度报告")).toBeNull();
});
it("does not open unsafe source URLs from old or imported records", () => {
  render(
    <SourceEvidence
      source={{ ...evidence[0], sourceUrl: "javascript:alert(1)" }}
    />,
  );
  fireEvent.click(screen.getByText("基金年度报告"));
  expect(screen.queryByRole("link")).toBeNull();
  expect(screen.getByText("来源链接不可用")).toBeTruthy();
});

it("does not offer confirmation for contradictory or unchecked advice", () => {
  render(
    <AdviceCards
      report={report}
      evidence={evidence}
      analysisId="a1"
      grounding={[
        {
          actionIndex: 0,
          status: "contradicted",
          reason: "建议与原始资料相反",
          checks: [],
        },
      ]}
      onCreateDecisionDraft={vi.fn()}
    />,
  );
  expect(screen.getByText("发现证据矛盾")).toBeTruthy();
  expect(screen.getByText("尚未完成证据核对")).toBeTruthy();
  expect(
    screen
      .getAllByRole("button", { name: "确认行动草稿" })
      .every((button) => (button as HTMLButtonElement).disabled),
  ).toBe(true);
});
