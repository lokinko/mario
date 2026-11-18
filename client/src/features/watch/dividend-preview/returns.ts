import market from "./market-snapshot.json";

export const priceOn = market.priceOn;
export const priceCapturedOn = market.capturedOn;
export const returnMethod =
  "按前复权收盘价计算区间累计涨跌，金额以人民币计，未计交易费用。";
export const yieldMethod =
  "股息率按近 365 天已实施税前每股派息除以不复权收盘价计算。";

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
