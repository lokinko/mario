import type { BandRule, WatchRules } from "../../api/watch";

export const referenceLabels = { lower: "下轨", middle: "中轨", upper: "上轨" };
export function bandRuleLabel(rule: BandRule) {
  const reference = referenceLabels[rule.reference];
  const pct = rule.offsetBps / 100;
  if (rule.mode === "near") return `${reference} ±${pct}%`;
  const mode = rule.mode === "at_or_below" ? "不高于" : "不低于";
  return `${mode}${reference}${pct ? `的 ${100 + pct}%` : ""}`;
}
export function rulesSummary(rules: WatchRules) {
  return `TTM税前股息率至少${rules.minYieldBps / 100}%，日线BOLL(20,2)${bandRuleLabel(rules.daily)}，周线BOLL(20,2)${bandRuleLabel(rules.weekly)}`;
}

/** Convert an edited percentage to integer basis points without floating point rounding. */
export function percentBps(value: string): number | null {
  if (!/^-?\d+(?:\.\d{1,2})?$/.test(value.trim())) return null;
  const [whole, fraction = ""] = value.trim().replace(/^-/, "").split(".");
  const bps = Number(whole) * 100 + Number(fraction.padEnd(2, "0"));
  return Number.isSafeInteger(bps)
    ? value.trim().startsWith("-")
      ? -bps
      : bps
    : null;
}
