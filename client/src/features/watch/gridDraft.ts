import type { WatchGridPlan, WatchRow } from "../../api/watch";

export interface PriceGridGenerator {
  basePrice: string;
  stepUnit: "pct" | "price";
  step: string;
  down: string;
  up: string;
  buySize: string;
  sellSize: string;
}

const priceInput = /^\d+(\.\d{1,2})?$/;
export const cents = (text: string) =>
  priceInput.test(text.trim()) ? Math.round(Number(text) * 100) : NaN;
export const priceText = (value: number) => (value / 100).toFixed(2);

/** The TTM dividend is usable only when the server published dividend reference prices. */
export const gridDividend = (row: WatchRow) =>
  row.dividendPrices.length && row.latest?.dividendPerShare
    ? Number(row.latest.dividendPerShare)
    : null;

/** Display aid only: the yield implied by the current TTM dividend per share. */
export function yieldAtPrice(dividend: number | null, price: string) {
  const value = cents(price);
  return dividend && value > 0 ? (dividend / value) * 10_000 : null;
}
export const yieldText = (dividend: number | null, price: string) => {
  const value = yieldAtPrice(dividend, price);
  return value == null ? "" : String(Number(value.toFixed(2)));
};
export function priceAtYield(dividend: number | null, yieldPct: string) {
  const value = Number(yieldPct);
  if (!dividend || !yieldPct.trim() || !(value > 0) || value > 100) return "";
  return priceText(Math.round((dividend / value) * 10_000));
}

// This creates editable price levels only. Shares, money and returns stay on the server.
export function generatePriceLevels(g: PriceGridGenerator) {
  const base = cents(g.basePrice);
  const step = Number(g.step);
  const down = Number(g.down);
  const up = Number(g.up);
  const stepCents = g.stepUnit === "price" ? cents(g.step) : NaN;
  if (
    !(base > 0) ||
    !Number.isInteger(down) ||
    !Number.isInteger(up) ||
    down < 1 ||
    up < 0 ||
    down + up > 100 ||
    (g.stepUnit === "pct" ? !(step > 0 && step < 100) : !(stepCents > 0))
  )
    throw new Error(
      "请填写基准价、大于 0 的档距（比例须小于 100%），向下至少 1 档，上下合计不超过 100 档。",
    );
  const at = (direction: 1 | -1, index: number) =>
    g.stepUnit === "pct"
      ? Math.round(base * (1 + (direction * step) / 100) ** index)
      : base + direction * index * stepCents;
  const side = (direction: 1 | -1, count: number, size: string) =>
    Array.from({ length: count }, (_, i) => at(direction, i + 1)).map(
      (price, i, prices) => {
        const previous = i ? prices[i - 1] : base;
        if (
          price <= 0 ||
          price > 100_000_000 ||
          direction * (price - previous) <= 0
        )
          throw new Error(
            "档距过小或档数过多，价格须在 0.01 元至 100 万元之间且互不重复。",
          );
        return { price: priceText(price), size };
      },
    );
  return [
    ...side(1, up, g.sellSize).reverse(),
    ...side(-1, down, g.buySize),
  ] satisfies WatchGridPlan["levels"];
}
