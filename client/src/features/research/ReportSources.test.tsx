// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type {
  AnalysisEvidenceReference,
  AnalysisWorkflowTrace,
} from "../../types";
import { reportSources, SourceDetail } from "./ReportSources";

afterEach(cleanup);
it("opens precise fact sheet values from the frozen report context", () => {
  const trace = {
    personalContext: {
      facts: {
        holdings: {
          asset: { name: "基金", marketValue: "900719925474099.91" },
        },
      },
    },
    structuredReport: {
      facts: [
        {
          statement: "基金市值",
          basis: "user_data",
          evidenceIds: [],
          sourceRefs: ["/facts/holdings/asset/marketValue"],
        },
      ],
      suggestions: [],
    },
  } as unknown as AnalysisWorkflowTrace;
  const result = reportSources(trace);
  expect(result.sources[0].claim).toBe("900719925474099.91");
  expect(result.sources[0].title).toBe("本次事实卡 · 基金 · 市值");
});

it("resolves original personal references even when evidence review is unavailable", () => {
  const trace = {
    personalContext: {
      currentUserMessage: "三年内用钱",
      portfolio: { totalValue: 120000 },
    },
    structuredReport: {
      facts: [
        {
          statement: "资金期限三年",
          basis: "user_data",
          evidenceIds: [],
          sourceRefs: ["/currentUserMessage"],
        },
      ],
      inferences: [
        {
          statement: "应预留现金",
          basis: "user_data",
          evidenceIds: [],
          sourceRefs: ["/portfolio/totalValue"],
        },
      ],
    },
  } as unknown as AnalysisWorkflowTrace;
  const result = reportSources(trace);
  expect(result.sources[0].title).toBe("本轮原始消息");
  expect(result.sources.map((source) => source.claim)).toEqual([
    "三年内用钱",
    "120000",
  ]);
  expect(result.localByFact.get(0)).toEqual(["local:/currentUserMessage"]);
  expect(result.localByInference.get(0)).toEqual([
    "local:/portfolio/totalValue",
  ]);
});

it("only exposes validated local originals and references actually used by the report", () => {
  const trace = {
    personalContext: {
      currentUserMessage: "我三年内需要用钱",
      portfolio: { holdings: [{ name: "存款", marketValue: 120000 }] },
    },
    evidenceCatalog: [{ id: "e1", title: "未使用的资料" }],
    structuredReport: {
      facts: [{ statement: "期限为三年", basis: "user_data", evidenceIds: [] }],
      inferences: [],
    },
    adviceGrounding: [
      {
        checks: [
          {
            factIndex: 0,
            sourceKind: "local",
            sourceRef: "/currentUserMessage",
            quote: "我三年内需要用钱",
          },
          {
            factIndex: 0,
            sourceKind: "local",
            sourceRef: "/portfolio/holdings/0/marketValue",
            quote: "120000.0",
          },
          {
            factIndex: 0,
            sourceKind: "local",
            sourceRef: "/portfolio/holdings/0/marketValue",
            quote: "12",
          },
          {
            factIndex: 0,
            sourceKind: "local",
            sourceRef: "/question",
            quote: "旧模型回答",
          },
        ],
      },
    ],
  } as unknown as AnalysisWorkflowTrace;
  const result = reportSources(trace);
  expect(result.sources.map((source) => source.claim)).toEqual([
    "我三年内需要用钱",
    "120000.0",
  ]);
  expect(result.sources[1].title).toBe("我的持仓 · 存款 · 市值");
  expect(result.localByFact.get(0)?.length).toBe(2);
});

it("shows saved user text without needing a URL or exposing metadata", () => {
  render(
    <SourceDetail
      regionId="source"
      close={() => {}}
      source={{
        id: "e1",
        title: "用户资料",
        claim: "预算更新为30万元。\n原始说明",
        publisher: "用户保存的原始资料",
        sourceUrl: "",
        evidenceType: "user_source_text",
        asOfDate: "",
        sourceTier: "待核实",
      }}
    />,
  );
  expect(screen.getByText(/预算更新为30万元/).textContent).toBe(
    "预算更新为30万元。\n原始说明",
  );
  expect(screen.queryByRole("link")).toBeNull();
});

it.each([
  "javascript:alert(1)",
  "file:///private",
  "https://user:secret@example.com",
])("does not create an unsafe source link: %s", (sourceUrl) => {
  render(
    <SourceDetail
      regionId="source"
      close={() => {}}
      source={
        {
          id: "e1",
          title: "旧来源",
          claim: "保存的正文",
          sourceUrl,
        } as AnalysisEvidenceReference
      }
    />,
  );
  expect(screen.queryByRole("link")).toBeNull();
});

it("keeps historical AI text visibly qualified and preserves multiline originals", () => {
  render(
    <SourceDetail
      regionId="source"
      close={() => {}}
      source={
        {
          id: "memory-a1",
          title: "历史问答",
          claim: JSON.stringify({ answer: "旧回答第一段\n第二段" }),
          sourceUrl: "",
          evidenceType: "historical_ai",
        } as AnalysisEvidenceReference
      }
    />,
  );
  expect(screen.getByText(/旧回答第一段/).textContent).toBe(
    "旧回答第一段\n第二段",
  );
  expect(screen.getByText(/未经结果验证/)).toBeTruthy();
});
