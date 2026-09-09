import type {
  FinancialProfile,
  Goal,
  Holding,
  PortfolioEventInput,
  PortfolioEventType,
} from "../../types";
import { localDateValue } from "../../lib/dates";

export const emptyProfile: FinancialProfile = {
  monthlyIncome: 0,
  monthlyExpense: 0,
  emergencyFund: 0,
  liabilities: 0,
  investableAssets: 0,
  horizonYears: 5,
  maxDrawdownPct: 15,
  riskLevel: "稳健",
  baseCurrency: "CNY",
};

export function emptyHolding(baseCurrency = "CNY"): Omit<Holding, "id"> {
  return {
    symbol: "",
    name: "",
    assetClass: "基金",
    marketValue: 0,
    costBasis: 0,
    targetPct: 0,
    currency: baseCurrency,
    fxRateToBase: null,
    valuationDate: localDateValue(new Date()),
    fxRateSource: "",
    fxRateObservedOn: "",
  };
}

export function emptyPortfolioEvent(baseCurrency = "CNY"): PortfolioEventInput {
  return {
    eventType: "deposit",
    source: "manual",
    externalId: "",
    assetName: "",
    amount: 0,
    currency: baseCurrency,
    fxRateToBase: null,
    fxRateSource: "",
    fxRateObservedOn: "",
    occurredOn: localDateValue(new Date()),
    note: "",
  };
}

export const portfolioEventLabels: Record<PortfolioEventType, string> = {
  deposit: "入金",
  withdrawal: "出金",
  dividend: "分红",
  interest: "利息",
  fee: "费用",
  tax: "税费",
  buy: "买入",
  sell: "卖出",
};

export function downloadPortfolioEventCsvTemplate(baseCurrency: string) {
  const portfolioEventCsvTemplate = `\ufeffsource,external_id,event_type,occurred_on,amount,currency,fx_rate_to_base,fx_rate_source,fx_rate_observed_on,asset_name,note\n券商账户,trade-001,buy,${localDateValue(new Date())},10000,${baseCurrency},,,,全球指数基金,定投买入\n`;
  const url = URL.createObjectURL(
    new Blob([portfolioEventCsvTemplate], { type: "text/csv;charset=utf-8" }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = "mario_组合流水模板.csv";
  link.click();
  URL.revokeObjectURL(url);
}

export function nextCalendarDate(value: string) {
  const date = new Date(`${value}T12:00:00`);
  date.setDate(date.getDate() + 1);
  return localDateValue(date);
}

export const emptyGoal: Omit<Goal, "id"> = {
  name: "",
  targetAmount: 0,
  currentAmount: 0,
  monthlyContribution: 0,
  targetDate: "",
  priority: "重要",
};
