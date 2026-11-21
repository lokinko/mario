// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type {
  AnalysisEvidenceReference,
  AnalysisWorkflowTrace,
} from "../../types/ai";
import { reportSources, SourceDetail } from "./ReportSources";

afterEach(cleanup);
it("groups holding fields into one personal summary without creating single-number footnotes", () => {
  const trace = {
    personalContext: {
      facts: {
        holdings: {
          asset: {
            name: "云铝股份",
            symbol: "000807.SZ",
            currency: "CNY",
            quantityUnit: "股",
            currentQuantity: "2200",
            marketValue: "900719925474099.91",
            weightPct: "12.5",
          },
        },
      },
    },
    structuredReport: {
      facts: [
        {
          statement: "云铝股份当前持股与市值",
          basis: "user_data",
          evidenceIds: [],
          sourceRefs: [
            "/facts/holdings/asset/name",
            "/facts/holdings/asset/symbol",
            "/facts/holdings/asset/currency",
            "/facts/holdings/asset/currentQuantity",
            "/facts/holdings/asset/marketValue",
            "/facts/holdings/asset/weightPct",
          ],
        },
      ],
      suggestions: [],
    },
  } as unknown as AnalysisWorkflowTrace;
  const result = reportSources(trace);
  expect(result.sources).toEqual([]);
  expect(result.personalFacts).toEqual([
    {
      id: "/facts/holdings/asset",
      title: "云铝股份（000807.SZ）",
      detail: "当前数量 2200 股 · 市值 900719925474099.91 CNY · 占比 12.5%",
    },
  ]);
});

it("shows original user corrections without promoting historical model text to facts", () => {
  const trace = {
    personalContext: {
      previousTurn: {
        question: "先核实哪两项？",
        earlierQuestions: ["这笔钱两年后可能要用"],
        answer: "MODEL_CONCLUSION",
        suggestions: ["MODEL_ACTION"],
        pendingQuestion: "MODEL_QUESTION",
      },
    },
    structuredReport: {
      facts: [
        {
          statement: "用户更正用款期限",
          basis: "user_data",
          evidenceIds: [],
          sourceRefs: [
            "/previousTurn/question",
            "/previousTurn/earlierQuestions/0",
            "/previousTurn/answer",
            "/previousTurn/suggestions/0",
            "/previousTurn/pendingQuestion",
            "/previousTurn/earlierQuestions",
            "/previousTurn/earlierQuestions/01",
          ],
        },
      ],
      suggestions: [],
    },
  } as unknown as AnalysisWorkflowTrace;
  const result = reportSources(trace);
  expect(result.sources.map((source) => source.claim)).toEqual([
    "先核实哪两项？",
    "这笔钱两年后可能要用",
  ]);
  expect(result.sources.map((source) => source.title)).toEqual([
    "上一轮用户原话",
    "早期用户原话 · 摘录 1",
  ]);
  expect(result.sources[1].sourceTier).toBe("用户声明（未改写档案）");
  render(
    <SourceDetail
      source={result.sources[1]}
      regionId="history-source"
      close={() => {}}
    />,
  );
  expect(screen.getByText("这笔钱两年后可能要用")).toBeTruthy();
  expect(screen.getByText("用户声明（未改写档案）。")).toBeTruthy();
  expect(screen.queryByText("MODEL_CONCLUSION")).toBeNull();
  expect(result.localByFact.get(0)).toEqual([
    "local:/previousTurn/question",
    "local:/previousTurn/earlierQuestions/0",
  ]);
});

it("identifies the company and transaction date behind an exact legacy trade reference", () => {
  const trace = {
    personalContext: {
      facts: {
        holdings: {
          asset: {
            name: "测试公司",
            recentTrades: {
              t1: { occurredOn: "2026-11-04", originalPrice: "12.34567890" },
            },
          },
        },
      },
    },
    structuredReport: {
      facts: [
        {
          statement: "原始成交价",
          basis: "user_data",
          evidenceIds: [],
          sourceRefs: ["/facts/holdings/asset/recentTrades/t1/originalPrice"],
        },
      ],
      suggestions: [],
    },
  } as unknown as AnalysisWorkflowTrace;
  const result = reportSources(trace);
  expect(result.sources).toEqual([]);
  expect(result.personalFacts).toEqual([
    {
      id: "/facts/holdings/asset:t1",
      title: "测试公司 · 2026-11-04",
      detail: "原始成交价 12.34567890",
    },
  ]);
});

it("omits current-turn question noise and groups personal portfolio totals", () => {
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
  expect(result.sources).toEqual([]);
  expect(result.personalFacts).toEqual([
    {
      id: "personal:summary",
      title: "个人财务摘要",
      detail: "资产总额 120000",
    },
  ]);
});

it("only exposes validated local summaries and references actually used by the report", () => {
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
  expect(result.sources).toEqual([]);
  expect(result.personalFacts).toEqual([
    {
      id: "/portfolio/holdings/0",
      title: "存款",
      detail: "市值 120000.0",
    },
  ]);
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
  expect(screen.queryByRole("button", { name: "收起原始信息" })).toBeNull();
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
