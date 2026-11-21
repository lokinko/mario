// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ModelReport } from "./ModelReport";
import type { AnalysisWorkflowTrace } from "../../types/ai";

afterEach(cleanup);

it("shows a concise answer, linked suggestions, citation drawer and copy action", () => {
  const trace = {
    version: "investment-workflow-v1",
    personalContext: {
      question: "每月结余8000元",
      facts: {
        summary: { baseCurrency: "CNY", monthlySurplus: "8000" },
      },
    },
    evidenceCatalog: [
      {
        id: "e1",
        title: "原始来源",
        publisher: "公司公告",
        sourceUrl: "https://example.com/source",
        sourceTier: "一手来源",
        claim: "原始资料全文\n第二段，保留原始换行。",
        capturedAt: "2026-09-20",
        asOfDate: "",
      },
      {
        id: "unused",
        title: "未引用候选",
        publisher: "",
        sourceUrl: "",
        sourceTier: "",
        asOfDate: "",
        claim: "未使用内容",
      },
    ],
    webSearch: {
      status: "incomplete",
      warnings: ["搜索未返回原文"],
      searchCalls: 0,
    },
    structuredReport: {
      answer: "本月先把结余转入备用金。",
      suggestions: [
        {
          action: "本月将8000元结余转入备用金",
          why: "每月结余8000元，且外部公告确认短期波动加大",
          factIndices: [0, 1],
          unlessWhen: "本月有必要的大额支出时调整",
        },
      ],
      facts: [
        {
          statement: "每月结余8000元",
          basis: "user_data",
          sourceRefs: ["/question", "/facts/summary/monthlySurplus"],
          evidenceIds: [],
        },
        {
          statement: "公告说明短期波动",
          basis: "research_evidence",
          sourceRefs: [],
          evidenceIds: ["e1"],
        },
      ],
      question: "负债何时需要偿还？",
    },
  } as unknown as AnalysisWorkflowTrace;

  render(<ModelReport answer="备用摘要" trace={trace} />);
  expect(screen.getByText("本月先把结余转入备用金。")).toBeTruthy();
  expect(screen.getByRole("list", { name: "建议" }).textContent).toContain(
    "本月将8000元结余转入备用金",
  );
  expect(screen.getByText("追问：负债何时需要偿还？")).toBeTruthy();

  const details = screen.getByText("依据与局限").closest("details")!;
  expect(details.open).toBe(false);
  expect(details.textContent).toContain("搜索未返回原文");
  expect(details.textContent).toContain("个人财务摘要");
  expect(details.textContent).toContain("月结余 8000 CNY");
  expect(screen.queryByText("未引用候选")).toBeNull();

  const citation = screen.getAllByRole("button", {
    name: "查看引用 1：原始来源",
  })[0];
  fireEvent.click(citation);
  expect(screen.getByRole("region", { name: "引用原始信息" })).toBeTruthy();
  expect(screen.getByText(/原始资料全文/).textContent).toBe(
    "原始资料全文\n第二段，保留原始换行。",
  );
  expect(
    screen
      .getByRole("link", { name: "打开来源网页：原始来源" })
      .getAttribute("href"),
  ).toBe("https://example.com/source");
  fireEvent.keyDown(screen.getByRole("region", { name: "引用原始信息" }), {
    key: "Escape",
  });
  expect(screen.queryByRole("region", { name: "引用原始信息" })).toBeNull();
  expect(document.activeElement).toBe(citation);
});

it("falls back to plain text when no structured report is present", () => {
  render(<ModelReport answer={"纯文本草稿\n第二段"} />);
  expect(screen.getByText(/纯文本草稿/).textContent).toBe("纯文本草稿\n第二段");
});

it("shows compact route metadata for fast-mode direct answers without module clutter", () => {
  const report = {
    answer: "当前应急金约覆盖 0.8 个月支出。",
    suggestions: [
      {
        action: "先核对最新月支出与现金余额",
        why: "基于当前授权摘要",
        factIndices: [0],
        unlessWhen: "",
      },
    ],
    facts: [
      {
        statement: "应急金约 0.8 个月",
        basis: "user_data" as const,
        sourceRefs: ["/facts/summary/emergencyMonths"],
        evidenceIds: [],
      },
    ],
    question: null,
  };
  const trace = {
    version: "investment-workflow-v1",
    calls: [],
    evidenceCatalog: [],
    structuredReport: report,
    route: {
      domain: "general_finance",
      domainLabel: "通用财务与资料核查",
      executionMode: "fast" as const,
      reason: "属于资料状态或口径核查类轻量问题，启用精简快速模式",
    },
  } as unknown as AnalysisWorkflowTrace;

  render(<ModelReport answer="" trace={trace} />);
  const meta = screen.getByLabelText("分析场景与模式");
  expect(meta.textContent).toContain("通用财务与资料核查");
  expect(meta.textContent).toContain("精简直答");
  expect(screen.queryByRole("region", { name: "分模块讨论" })).toBeNull();
});

it("renders scannable specialist module stances with progressive disclosure details and copy support", () => {
  const report = {
    answer:
      "云铝股份当前维持底仓观察，暂不因浮亏加仓，等铝价回调或成本端企稳后再按左侧条件分批介入。",
    suggestions: [
      {
        action:
          "维持现有 4.23% 仓位观察，若股价回落至周期中枢 PB 低分位再分两批加仓",
        why: "现有仓位已具参与弹性，浮亏补仓须满足基本面周期底部与赔率改善条件",
        factIndices: [0],
        unlessWhen: "氧化铝大幅涨价导致吨铝毛利转弱且云南限电减产时中止加仓",
      },
    ],
    facts: [
      {
        statement: "当前持仓占比 4.2316%",
        basis: "user_data" as const,
        sourceRefs: ["/facts/holdings/ylgf/weightPct"],
        evidenceIds: [],
      },
    ],
    question: null,
  };
  const trace = {
    version: "investment-workflow-v1",
    calls: [],
    evidenceCatalog: [],
    structuredReport: report,
    route: {
      domain: "equity_valuation",
      domainLabel: "个股估值与基本面",
      executionMode: "council" as const,
      reason: "属于投资决策或深度投研问题，启动分模块专业分析",
      companies: ["000807.SZ"],
    },
    research: {
      brief: {
        findings: [],
        counterpoints: [],
        unknowns: ["最新季度吨铝完全成本"],
        checks: [
          {
            index: 0,
            status: "covered" as const,
            note: "已核对授权持仓占比",
          },
        ],
      },
    },
    modules: [
      {
        id: "M1",
        module: "fundamentals_valuation",
        label: "基本面与估值定价",
        analysis: {
          stance: "水电铝成本优势突出，但须防按周期高点盈利误判低估",
          summary: "云铝受益绿电成本优势，当前更需关注吨铝价差与周期中枢 PB。",
          keyPoints: [
            "水电铝具备长期成本曲线优势",
            "周期股估值以周期中枢盈利和PB分位为准",
          ],
          triggers: ["铝价回落至行业高成本线附近且PB回落至历史低位"],
          dataGaps: [],
          facts: [],
        },
      },
      {
        id: "M2",
        module: "portfolio_execution",
        label: "组合策略与交易执行",
        analysis: {
          stance: "维持 4.23% 底仓，暂不因浮亏补仓",
          summary: "组合已有敞口，不因成本价倒挂急于加仓。",
          keyPoints: ["单标的仓位处于合理观察区"],
          triggers: ["触发估值条件后按两批等额介入"],
          dataGaps: [],
          facts: [],
        },
      },
      {
        id: "M3",
        module: "risk_bear_case",
        label: "风险压力与反方证伪",
        failure: "格式校验失败",
      },
    ],
    synthesis: {
      status: "degraded",
      modulesUsed: 2,
      modulesTotal: 3,
    },
    webSearch: {
      status: "incomplete",
      searchCalls: 1,
      warnings: ["本次未取得可引用的网页原文；外部信息仍待核实"],
      leads: [
        {
          title: "网页检索综合摘要（公开线索，待公告复核）",
          url: "",
          summary: "沪铝主力震荡运行，云铝绿电铝产能占比高。",
        },
        {
          title: "行业简报",
          url: "https://example.com/lead",
          summary: "云南枯水期电力供应跟踪。",
        },
      ],
    },
  } as unknown as AnalysisWorkflowTrace;

  render(<ModelReport answer="" trace={trace} />);

  const meta = screen.getByLabelText("分析场景与模式");
  expect(meta.textContent).toContain("个股估值与基本面");
  expect(meta.textContent).toContain("分模块策略汇总");
  expect(meta.textContent).toContain("000807.SZ");

  // Default scannable layer: module labels & one-line stances visible immediately.
  const breakdown = screen.getByRole("region", { name: "分模块讨论" });
  expect(breakdown.textContent).toContain("2/3 个专业模块 · 部分降级");
  expect(breakdown.textContent).toContain("基本面与估值定价");
  expect(breakdown.textContent).toContain(
    "水电铝成本优势突出，但须防按周期高点盈利误判低估",
  );
  expect(breakdown.textContent).toContain("组合策略与交易执行");
  expect(breakdown.textContent).toContain("维持 4.23% 底仓，暂不因浮亏补仓");
  expect(breakdown.textContent).toContain("未完成（格式校验失败）");

  // Detailed module reasoning stays folded until opened.
  const detailElements = breakdown.querySelectorAll("details");
  expect(detailElements.length).toBe(2);
  expect(detailElements[0].open).toBe(false);

  // Research checks and web leads stay cleanly folded below.
  const researchDetails = screen
    .getByText(/研究核查与网页线索/)
    .closest("details")!;
  expect(researchDetails.open).toBe(false);
  expect(researchDetails.textContent).toContain("沪铝主力震荡运行");
  expect(
    screen.getByRole("link", { name: "行业简报" }).getAttribute("href"),
  ).toBe("https://example.com/lead");

  // Copy output includes summary, suggestions, and module stances.
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
  fireEvent.click(screen.getByRole("button", { name: "复制回答" }));
  expect(writeText).toHaveBeenCalledTimes(1);
  const copied = writeText.mock.calls[0][0] as string;
  expect(copied).toContain("云铝股份当前维持底仓观察");
  expect(copied).toContain("分模块立场：");
  expect(copied).toContain("基本面与估值定价：水电铝成本优势突出");
});
