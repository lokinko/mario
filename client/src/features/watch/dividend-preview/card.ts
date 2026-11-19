import { dividendBriefs } from "./briefs";
import { dividendReturns, priceOn } from "./returns";
import type snapshot from "./snapshot.json";

type DividendSample = (typeof snapshot.stocks)[number];

type DividendMetric = {
  label: string;
  value: string;
  note: string;
  date?: { label: string; value: string };
};

function priceCommentary(year: { from?: string; value: string } | undefined) {
  if (!year?.from) {
    return "近一年股价历史不足，现有数据可用于观察现金派息。";
  }
  if (year.value.startsWith("+")) {
    return "近一年前复权股价上涨。派息金额相同的情况下，买入价越高，对应的股息率越低。";
  }
  if (year.value.startsWith("-")) {
    return "近一年前复权股价下跌。若派息金额持平，较低的买入价对应较高的股息率。评估持有体验时，还要结合这段期间的价格跌幅。";
  }
  return "近一年前复权股价大致持平，可单独参考股息率衡量现金分配水平。";
}

export function dividendCard(stock: DividendSample) {
  const [latest] = stock.events;
  if (!latest) return;

  const { name, symbol } = stock;
  const brief = dividendBriefs[symbol];
  const market = dividendReturns(symbol);
  const year = market?.periods[0];
  const metrics: DividendMetric[] = [
    {
      label: "税前股息率",
      value: market?.yieldPct ?? "暂无数据",
      note: "近 365 天",
    },
    {
      label: "前复权涨跌",
      value: year?.value ?? "暂无数据",
      note: "近 1 年",
      date: market && { label: "截至", value: priceOn },
    },
    {
      label: "最近派息",
      value: `${latest.cashPerTen} 元`,
      note: "每 10 股（税前）",
      date: { label: "除息", value: latest.exDate },
    },
  ];

  const performance = {
    kind: "performance" as const,
    title: "分红与近一年股价",
    text: brief
      ? `${brief.payout}${priceCommentary(year)}`
      : priceCommentary(year),
    metrics,
  };

  const sections = brief
    ? ([
        { kind: "outlook", title: "未来预期", text: brief.outlook },
        { kind: "drivers", title: "影响股息的关键数据", text: brief.drivers },
        performance,
      ] as const)
    : ([performance] as const);

  return { name, symbol, sections };
}

export type DividendCard = NonNullable<ReturnType<typeof dividendCard>>;
