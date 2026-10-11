// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { ModelReport } from "./ModelReport";
import type { AnalysisWorkflowTrace } from "../../types";

afterEach(cleanup);
it("shows the complete report with sources and limitations, without process or action controls", () => {
  const trace = {
    researchPlan: "内部研究计划",
    evidenceCatalog: [
      { id: "e1", title: "原始来源", sourceUrl: "https://example.com/source" },
    ],
    structuredReport: {
      verdict: "优先补足备用金",
      facts: [
        { statement: "备用金不足", basis: "user_data", evidenceIds: [] },
        {
          statement: "资料事实",
          basis: "research_evidence",
          evidenceIds: ["e1"],
        },
      ],
      inferences: [
        { statement: "现金缓冲较少", basis: "user_data", evidenceIds: [] },
      ],
      unknowns: ["需核实近期支出"],
      options: [
        {
          name: "先储蓄",
          suitableWhen: "近期用钱",
          tradeoffs: ["延后投资"],
          risks: ["通胀风险"],
        },
      ],
      actions: [
        {
          action: "补足现金",
          rationale: "减少短期风险",
          supportingFactIndices: [0],
          evidenceLimits: ["支出数据尚未更新"],
          reversible: true,
          reviewTrigger: "月底核对",
        },
      ],
      reviewTriggers: ["收入改变时复盘"],
    },
  } as unknown as AnalysisWorkflowTrace;
  render(<ModelReport answer="旧摘要" trace={trace} />);
  for (const content of [
    "优先补足备用金",
    "现金缓冲较少",
    "需核实近期支出",
    "先储蓄",
    "补足现金",
    "收入改变时复盘",
  ])
    expect(screen.getByText(content)).toBeTruthy();
  expect(screen.getByText(/支出数据尚未更新/)).toBeTruthy();
  expect(
    screen.getByRole("link", { name: "原始来源" }).getAttribute("href"),
  ).toBe("https://example.com/source");
  expect(screen.queryByText("内部研究计划")).toBeNull();
  expect(screen.queryByText("旧摘要")).toBeNull();
  expect(screen.queryByRole("button")).toBeNull();
});
it("shows the original answer for older records without a structured report", () => {
  render(<ModelReport answer={"旧版完整回答\n第二段"} />);
  expect(screen.getByText(/旧版完整回答/).textContent).toBe(
    "旧版完整回答\n第二段",
  );
});
