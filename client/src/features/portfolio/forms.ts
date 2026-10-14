import type { Holding } from "../../types";
import { localDateValue } from "../../lib/dates";

export function emptyHolding(baseCurrency = "CNY"): Omit<Holding, "id"> {
  return {
    symbol: "",
    name: "",
    assetClass: "基金",
    marketValue: 0,
    costBasis: 0,
    costPrice: null,
    quantity: null,
    currentPrice: null,
    targetPct: 0,
    currency: baseCurrency,
    fxRateToBase: null,
    valuationDate: localDateValue(new Date()),
    fxRateSource: "",
    fxRateObservedOn: "",
  };
}
