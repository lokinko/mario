// @vitest-environment jsdom
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { ContextPreview } from "./ContextPreview";
import type { AnalysisPreview } from "../../types/ai";

afterEach(cleanup);

it("renders financial summary, sorted holdings table, evidence and memory in human-readable form", () => {
  const preview: AnalysisPreview = {
    provider: "openai-responses",
    model: "gpt-5.4",
    workflow: "council",
    generatedAt: "2026-11-21T09:18:55Z",
    contextRevision: "ctx-1",
    payloadBytes: 512,
    systemPolicy: "",
    localOnly: [],
    evidenceCandidates: [],
    groups: [
      {
        key: "investmentMethodology",
        label: "投资方法论",
        included: true,
        recordCount: 1,
        sensitivity: "个人设定",
        description: "",
      },
      {
        key: "question",
        label: "本次问题",
        included: true,
        recordCount: 1,
        sensitivity: "中",
        description: "",
      },
      {
        key: "profile",
        label: "财务档案",
        included: true,
        recordCount: 1,
        sensitivity: "高",
        description: "",
      },
      {
        key: "holdings",
        label: "当前持仓状态",
        included: true,
        recordCount: 2,
        sensitivity: "高",
        description: "",
      },
      {
        key: "evidence",
        label: "相关研究证据",
        included: true,
        recordCount: 1,
        sensitivity: "外部",
        description: "",
      },
      {
        key: "memory",
        label: "长期记忆",
        included: false,
        recordCount: 0,
        sensitivity: "个人设定",
        description: "",
      },
    ],
    payload: {
      question: "检查组合集中度",
      investmentMethodology: "重视现金流与安全边际",
      facts: {
        summary: {
          baseCurrency: "CNY",
          portfolioValue: "1092305.1603140772542556131382",
          cashValue: "555268.85017421602787456445993",
          emergencyFund: "80000",
          emergencyMonths: "11.43",
          maxDrawdownPct: "20",
        },
        holdings: {
          small: {
            name: "长江电力",
            symbol: "600900.SS",
            currency: "CNY",
            currentQuantity: "169.03",
            quantityUnit: "股",
            costBasis: "4891.80003775",
            marketValue: "5344.728741855621602",
            unrealizedProfit: "452.928704105621602",
            weightPct: "0.49",
          },
          large: {
            name: "腾讯",
            symbol: "00700.HK",
            currency: "HKD",
            currentQuantity: "300",
            quantityUnit: "股",
            costBasis: "134952.33",
            marketValue: "103200",
            unrealizedProfit: "-31752.33",
            weightPct: "8.04",
          },
        },
      },
      researchEvidence: [
        {
          id: "e1",
          title: "腾讯季报摘录",
          assetName: "腾讯",
          asOfDate: "2026-11-01",
          claim: "回购持续推进",
        },
      ],
    },
  };

  render(<ContextPreview preview={preview} previousQuestionCount={0} />);

  expect(screen.getByText("检查组合集中度")).toBeTruthy();
  expect(screen.getByText("¥1,092,305.16")).toBeTruthy();
  expect(screen.getByText("¥555,268.85")).toBeTruthy();
  expect(screen.getByText("¥80,000.00（11.43 个月）")).toBeTruthy();
  expect(screen.getByText("20%")).toBeTruthy();

  const table = screen.getByRole("table");
  const rows = within(table).getAllByRole("row");
  // Header + 2 holdings sorted by weight descending (腾讯 8.04% before 长江电力 0.49%).
  expect(rows).toHaveLength(3);
  expect(rows[1].textContent).toContain("腾讯");
  expect(rows[1].textContent).toContain("300 股");
  expect(rows[1].textContent).toContain("−31,752.33");
  expect(rows[1].textContent).toContain("8.04%");
  expect(rows[2].textContent).toContain("长江电力");
  expect(rows[2].textContent).toContain("4,891.80");
  expect(rows[2].textContent).toContain("+452.93");

  expect(screen.getByText("腾讯季报摘录")).toBeTruthy();
  expect(screen.getByText("回购持续推进")).toBeTruthy();
  expect(screen.getByText("投资方法论").closest("details")!.open).toBe(false);
  expect(screen.queryByText(/baseCurrency/)).toBeNull();
  expect(screen.queryByText(/\/facts\/holdings/)).toBeNull();
  expect(screen.queryByText(/· 0 条/)).toBeNull();
});
