// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ModelReport } from "./ModelReport";
import type { AnalysisWorkflowTrace } from "../../types/ai";

afterEach(cleanup);
it("shows a concise answer, linked suggestions and one question with folded evidence", () => {
  const trace = {
    personalContext: {
      question: "每月结余8000元",
      facts: {
        summary: { baseCurrency: "CNY", monthlySurplus: "8000" },
      },
    },
    evidenceCatalog: [],
    structuredReport: {
      answer: "本月先把结余转入备用金。",
      suggestions: [
        {
          action: "本月将8000元结余转入备用金",
          why: "每月结余8000元",
          factIndices: [0],
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
  expect(screen.queryByRole("heading", { name: "方案比较" })).toBeNull();
  const details = screen.getByText("依据与局限").closest("details")!;
  expect(details.open).toBe(false);
  expect(screen.queryByRole("button", { name: /查看引用/ })).toBeNull();
  expect(details.textContent).toContain("个人财务摘要");
  expect(details.textContent).toContain("月结余 8000 CNY");
});

it("shows the complete report with sources and limitations, without process or action controls", () => {
  const trace = {
    webSearch: {
      status: "incomplete",
      warnings: ["搜索未返回原文"],
      searchCalls: 0,
    },
    evidenceCatalog: [
      {
        id: "e1",
        title: "原始来源",
        sourceUrl: "https://example.com/source",
        claim: "原始资料全文\n第二段，保留原始换行。",
        capturedAt: "2026-09-20",
        asOfDate: "",
      },
      { id: "unused", title: "未引用候选", claim: "未使用内容" },
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
  const evidence = screen.getByText("依据与局限").closest("details")!;
  expect(evidence.open).toBe(false);
  expect(evidence.textContent).toContain("搜索未返回原文");
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
  expect(screen.queryByText(/原始资料全文/)).toBeNull();
  expect(screen.queryByText("未引用候选")).toBeNull();
  const citation = screen.getAllByRole("button", {
    name: "查看引用 1：原始来源",
  })[0];
  fireEvent.click(citation);
  expect(screen.getByRole("region", { name: "引用原始信息" })).toBeTruthy();
  expect(screen.getByText(/原始资料全文/).textContent).toBe(
    "原始资料全文\n第二段，保留原始换行。",
  );
  expect(screen.queryByText("资料日期：2026-09-20")).toBeNull();
  expect(
    screen
      .getByRole("link", { name: "打开来源网页：原始来源" })
      .getAttribute("href"),
  ).toBe("https://example.com/source");
  expect(screen.queryByText("旧摘要")).toBeNull();
  fireEvent.keyDown(screen.getByRole("region", { name: "引用原始信息" }), {
    key: "Escape",
  });
  expect(screen.queryByRole("region", { name: "引用原始信息" })).toBeNull();
  expect(document.activeElement).toBe(citation);
});
it("shows the original answer for older records without a structured report", () => {
  render(<ModelReport answer={"旧版完整回答\n第二段"} />);
  expect(screen.getByText(/旧版完整回答/).textContent).toBe(
    "旧版完整回答\n第二段",
  );
});

it("shows objections to the adopted proposal and keeps web leads out of the cited sources", () => {
  const report = {
    answer: "继续等待日线回到下轨附近。",
    suggestions: [],
    facts: [],
    question: null,
  };
  const scores = {
    facts: 2,
    citations: 2,
    reasoning: 2,
    counterpoints: 1,
    actionability: 1,
  };
  const trace = {
    evidenceCatalog: [],
    structuredReport: report,
    webSearch: {
      status: "completed",
      searchCalls: 1,
      warnings: [],
      leads: [
        {
          title: "分红新闻",
          url: "https://example.com/lead",
          summary: "供应商对分红的概括",
        },
      ],
    },
    research: {
      brief: {
        findings: [],
        counterpoints: [],
        unknowns: ["2025 年派息方案未公告"],
        checks: [
          { index: 0, status: "missing_source", note: "缺少近三年现金流" },
        ],
      },
    },
    proposals: [
      { id: "P1", lens: "steady", label: "稳健等待", report },
      { id: "P2", lens: "conditional", label: "条件执行", report },
      { id: "P3", lens: "skeptic", label: "反方证伪", failure: "格式无效" },
    ],
    vote: {
      status: "voted",
      winner: "P1",
      tally: [
        {
          proposal: "P1",
          voters: 1,
          preferences: 1,
          vetoes: 0,
          scoreTotal: 8,
          eliminated: false,
          rank: 1,
        },
        {
          proposal: "P2",
          voters: 1,
          preferences: 0,
          vetoes: 1,
          scoreTotal: 4,
          eliminated: true,
          rank: 2,
        },
      ],
      ballots: [
        {
          voter: "P2",
          lens: "conditional",
          ballot: {
            preferred: "P1",
            assessments: [
              {
                proposal: "P1",
                scores,
                veto: null,
                objection: "未说明等待期间的机会成本",
              },
            ],
          },
        },
      ],
    },
  } as unknown as AnalysisWorkflowTrace;
  render(<ModelReport answer="" trace={trace} />);
  const objections = screen.getByRole("region", {
    name: "其他方案代理的质疑",
  });
  expect(objections.textContent).toContain("未说明等待期间的机会成本");
  expect(objections.textContent).toContain("来自 条件执行");
  const process =
    screen.getByText(/评议过程：交叉评审后采纳 稳健等待/).parentElement!;
  expect(process.textContent).toContain(
    "稳健等待（采纳）：1/1 票首选，平均 8.0 / 10 分",
  );
  expect(process.textContent).toContain("1 票否决，已淘汰");
  expect(process.textContent).toContain("反方证伪：未通过结构化校验");
  expect(process.textContent).toContain("2025 年派息方案未公告");
  expect(process.textContent).toContain("缺少近三年现金流");
  expect(process.textContent).toContain("未作为回答依据");
  expect(
    screen.getByRole("link", { name: "分红新闻" }).getAttribute("href"),
  ).toBe("https://example.com/lead");
  expect(screen.getByText("本次回答未提供可核对的引用来源。")).toBeTruthy();

  cleanup();
  render(
    <ModelReport
      answer=""
      trace={{ ...trace, vote: { ...trace.vote!, status: "tied" } }}
    />,
  );
  // A tie decided by lens order must not read as a preference for the adopted proposal.
  expect(
    screen.getByText(
      /评议过程：评审首选与评分持平，未分出高下，按方案顺序采纳 稳健等待/,
    ),
  ).toBeTruthy();
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
  fireEvent.click(screen.getByRole("button", { name: "复制回答" }));
  expect(writeText).toHaveBeenCalledWith("继续等待日线回到下轨附近。");
});

it("shows the routed domain and fast-mode path without fabricating a vote", () => {
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
    version: "investment-workflow-v19",
    calls: [],
    evidenceCatalog: [],
    structuredReport: report,
    route: {
      domain: "general_finance",
      domainLabel: "通用财务与资料核查",
      executionMode: "fast" as const,
      reason: "属于资料状态或口径核查类轻量问题，启用精简快速模式",
    },
    proposals: [{ id: "DIRECT", lens: "direct", label: "精简直答", report }],
    vote: null,
  } as unknown as AnalysisWorkflowTrace;

  render(<ModelReport answer="" trace={trace} />);
  expect(
    screen.getByText(/分析路径：通用财务与资料核查 · 精简快速模式/),
  ).toBeTruthy();
  expect(
    screen.getByText("属于资料状态或口径核查类轻量问题，启用精简快速模式"),
  ).toBeTruthy();
  expect(
    screen.queryByRole("region", { name: "其他方案代理的质疑" }),
  ).toBeNull();
});

it("renders specialist modules and strategist synthesis summary for modular workflow traces", () => {
  const report = {
    answer:
      "【核心结论】云铝股份当前维持底仓观察，暂不因浮亏加仓，等铝价回调或成本端企稳后再按左侧条件分批介入。",
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
    version: "investment-workflow-v20",
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
        analysis: {
          stance: "警惕氧化铝涨价与枯水期限电双重挤压",
          summary: "若铝价回落叠加枯水期电价上行，吨铝利润弹性会迅速收敛。",
          keyPoints: ["枯水期云南限电对产销量构成扰动"],
          triggers: ["氧化铝持续涨价且铝价跌破成本支撑时复核仓位"],
          dataGaps: [],
          facts: [],
        },
      },
    ],
    synthesis: {
      status: "completed",
      modulesUsed: 3,
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
      ],
    },
  } as unknown as AnalysisWorkflowTrace;

  render(<ModelReport answer="" trace={trace} />);
  expect(
    screen.getByText(
      /分模块讨论与策略汇总：3\/3 个专业模块 · 个股估值与基本面/,
    ),
  ).toBeTruthy();
  const process = screen.getByText(/分模块讨论与策略汇总/).closest("details")!;
  expect(process.textContent).toContain("M1 · 基本面与估值定价");
  expect(process.textContent).toContain("M2 · 组合策略与交易执行");
  expect(process.textContent).toContain("M3 · 风险压力与反方证伪");
  expect(process.textContent).toContain("涉及公开标的：000807.SZ");
  expect(process.textContent).toContain("沪铝主力震荡运行");
});
