import type { FinancialProfile, Holding, ProfileField } from "../../types";
import { localDateValue } from "../../lib/dates";

export const profileFields: ProfileField[] = [
  "monthlyIncome",
  "monthlyExpense",
  "emergencyFund",
  "liabilities",
  "horizonYears",
  "maxDrawdownPct",
];

export function profileFieldProvided(
  profile: FinancialProfile,
  key: ProfileField,
) {
  if (profile.providedFields) return profile.providedFields.includes(key);
  // Old profiles did not record presence. Match the conservative fact-card rule.
  const value = profile[key];
  return (
    value > 0 &&
    !(key === "horizonYears" && value === 5) &&
    !(key === "maxDrawdownPct" && value === 15)
  );
}

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
