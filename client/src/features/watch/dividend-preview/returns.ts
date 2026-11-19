import market from "./market-snapshot.json";

export const priceOn = market.priceOn;

const yieldFormat = new Intl.NumberFormat("zh-CN", {
  style: "percent",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});
const returnFormat = new Intl.NumberFormat("zh-CN", {
  style: "percent",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
  signDisplay: "exceptZero",
});

export function dividendReturns(symbol: string) {
  const row = market.stocks.find((item) => item.symbol === symbol);
  if (!row) return undefined;

  return {
    ...row,
    yieldPct: yieldFormat.format(
      Number(row.dividendPerShare) / Number(row.close),
    ),
    periods: [1, 3, 5].map((years, index) => {
      const start = row.starts[index];
      return {
        label: `近 ${years} 年`,
        from: start?.day,
        value: start
          ? returnFormat.format(
              Number(row.adjustedClose) / Number(start.close) - 1,
            )
          : "历史不足",
      };
    }),
  };
}
