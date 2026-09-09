import type { Holding } from "../../types";

export function holdingValueInBase(holding: Holding, baseCurrency: string) {
  if (holding.currency === baseCurrency) return holding.marketValue;
  return holding.fxRateToBase ? holding.marketValue * holding.fxRateToBase : 0;
}

export const ecbFxMethodologyUrl =
  "https://data.ecb.europa.eu/key-figures/ecb-interest-rates-and-exchange-rates/exchange-rates";

export function fxSourceLabel(source: string) {
  if (source === "ecb_reference") return "ECB 参考汇率";
  if (source === "user_declared") return "用户声明汇率";
  return source || "待确认来源";
}

export function normalizedQuoteRate(rate: number) {
  return Number(rate.toPrecision(12));
}
