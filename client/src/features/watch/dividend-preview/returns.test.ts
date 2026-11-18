import { expect, it } from "vitest";
import market from "./market-snapshot.json";
import { wrapPosterText } from "./poster";
import { dividendReturns, priceOn } from "./returns";
import snapshot from "./snapshot.json";

it("uses the same dated snapshot for every sample and preserves unavailable periods", () => {
  expect(market.stocks.map(({ symbol }) => symbol)).toEqual(
    snapshot.stocks.map(({ symbol }) => symbol),
  );
  expect(priceOn).toBe("2026-11-06");
  const power = dividendReturns("600900");
  expect(power?.yieldPct).toBe("3.34%");
  expect(power?.periods.map(({ value }) => value)).toEqual([
    "+8.76%",
    "+53.54%",
    "+84.29%",
  ]);
  expect(dividendReturns("600941")?.periods[2]).toEqual({
    label: "近 5 年",
    from: undefined,
    value: "历史不足",
  });
});

it("preserves Chinese reading text and keeps decimal amounts together across lines", () => {
  const text = "每10股派7.33元。查年报里的经营现金流。";
  const lines = wrapPosterText(text, (line) => line.length <= 7);
  expect(lines.join("")).toBe(text);
  expect(lines.some((line) => line.includes("7.33"))).toBe(true);
  expect(lines.every((line) => !line.startsWith("。"))).toBe(true);
});
