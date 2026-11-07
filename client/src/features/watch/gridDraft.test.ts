import { expect, it } from "vitest";
import { generateGridLevels } from "./gridDraft";

const generator = {
  startYieldBps: 375,
  endYieldBps: 675,
  stepBps: 75,
  initialBuyShares: 100,
  initialSellShares: 50,
  buyGrowthBps: 5000,
  sellGrowthBps: 10_000,
};
it("generates custom spacing and share growth in the two execution directions", () => {
  const levels = generateGridLevels(generator);
  expect(levels.map((l) => l.yieldBps)).toEqual([375, 450, 525, 600, 675]);
  expect(levels.map((l) => l.buyShares)).toEqual([0, 100, 150, 225, 338]);
  expect(levels.map((l) => l.sellShares)).toEqual([400, 200, 100, 50, 0]);
  expect(
    generateGridLevels({ ...generator, buyGrowthBps: -5000 }).map(
      (l) => l.buyShares,
    ),
  ).toEqual([0, 100, 50, 25, 13]);
});
it("rejects truncated ranges, excessive levels, fractional shares and overflow", () => {
  for (const change of [
    { stepBps: 70 },
    { stepBps: 1 },
    { startYieldBps: 0 },
    { initialBuyShares: 1.5 },
    { buyGrowthBps: -10001 },
    { initialBuyShares: 1e9, buyGrowthBps: 1000 },
  ]) {
    expect(() => generateGridLevels({ ...generator, ...change })).toThrow();
  }
});
