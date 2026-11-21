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

it("omits current-turn question noise and groups financial summary fields", () => {
  const trace = {
    personalContext: {
      question: "三年内用钱",
      facts: {
        summary: { baseCurrency: "CNY", portfolioValue: "120000" },
      },
    },
    structuredReport: {
      facts: [
        {
          statement: "资金期限三年，组合总值12万",
          basis: "user_data",
          evidenceIds: [],
          sourceRefs: ["/question", "/facts/summary/portfolioValue"],
        },
      ],
      suggestions: [],
    },
  } as unknown as AnalysisWorkflowTrace;
  const result = reportSources(trace);
  expect(result.sources).toEqual([]);
  expect(result.personalFacts).toEqual([
    {
      id: "personal:summary",
      title: "个人财务摘要",
      detail: "资产总额 120000 CNY",
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
});

it("refuses unsafe source URLs and allows clean http/https links", () => {
  const base: AnalysisEvidenceReference = {
    id: "e1",
    title: "来源",
    publisher: "",
    sourceUrl: "javascript:alert(1)",
    sourceTier: "",
    asOfDate: "",
    claim: "正文",
  };
  render(<SourceDetail regionId="s1" close={() => {}} source={base} />);
  expect(screen.queryByRole("link")).toBeNull();
  cleanup();
  render(
    <SourceDetail
      regionId="s2"
      close={() => {}}
      source={{ ...base, sourceUrl: "https://example.com/report" }}
    />,
  );
  expect(
    screen
      .getByRole("link", { name: "打开来源网页：来源" })
      .getAttribute("href"),
  ).toBe("https://example.com/report");
});
