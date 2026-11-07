import type { WatchGridGenerator, WatchGridPlan } from "../../api/watch";

export const yieldLabel = (value: number) =>
  `${Number((value / 100).toFixed(2))}%`;
export const maxShares = 1_000_000_000;

// This creates editable instructions only. Money and scenario calculations stay on the server.
export function generateGridLevels(
  g: WatchGridGenerator,
): WatchGridPlan["levels"] {
  const span = g.endYieldBps - g.startYieldBps;
  if (
    !Object.values(g).every(Number.isInteger) ||
    g.startYieldBps < 1 ||
    g.endYieldBps > 10_000 ||
    span <= 0 ||
    g.stepBps < 1 ||
    span % g.stepBps !== 0 ||
    span / g.stepBps >= 100 ||
    g.initialBuyShares < 0 ||
    g.initialSellShares < 0 ||
    g.initialBuyShares > maxShares ||
    g.initialSellShares > maxShares ||
    g.buyGrowthBps < -10_000 ||
    g.buyGrowthBps > 100_000 ||
    g.sellGrowthBps < -10_000 ||
    g.sellGrowthBps > 100_000
  )
    throw new Error(
      "请检查范围和间距，间距须整除范围，最多生成 100 档；递增比例为 -100% 至 1000%。",
    );
  const count = span / g.stepBps + 1;
  const quantity = (base: number, growth: number, index: number) => {
    const shares = Math.round(base * (1 + growth / 10_000) ** index);
    if (!Number.isSafeInteger(shares) || shares > maxShares)
      throw new Error(
        "生成后的每档股数不能超过 10 亿，请减少初始股数或递增比例。",
      );
    return shares;
  };
  return Array.from({ length: count }, (_, i) => ({
    yieldBps: g.startYieldBps + i * g.stepBps,
    // The low endpoint is the rebound target; generation starts buying one step above it.
    buyShares:
      i === 0 ? 0 : quantity(g.initialBuyShares, g.buyGrowthBps, i - 1),
    sellShares:
      i === count - 1
        ? 0
        : quantity(g.initialSellShares, g.sellGrowthBps, count - 2 - i),
  }));
}
