import { Money } from "../../components/AmountPrivacy";
import { Callout } from "../../components/Callout";
import { Card } from "antd";
import { Badge } from "../../components/Badge";
import type { Holding, Snapshot } from "../../types/user";
import { formatMoney, formatPercent, formatSigned } from "../../lib/format";
import { portfolioOverview, overviewMetrics } from "./portfolioOverview";

const labels = {
  value: "持仓市值",
  cost: "持仓成本",
  unrealized: "浮动盈亏",
  realized: "已结算盈亏",
};

export function InvestmentOverview({
  holdings,
  baseCurrency = "CNY",
  cash = false,
  accounting,
}: {
  holdings: Holding[];
  baseCurrency?: string;
  cash?: boolean;
  accounting?: Snapshot["accounting"];
}) {
  if (!accounting)
    return (
      <section aria-label={cash ? "现金概览" : "投资持仓概览"}>
        <Callout tone="danger">缺少精确会计数据，请刷新页面。</Callout>
      </section>
    );
  const { totals } = portfolioOverview(holdings, accounting, cash);
  const metrics = cash ? (["value"] as const) : overviewMetrics;
  const totalAssets =
    accounting.totalValue != null && Number(accounting.totalValue) > 0
      ? Number(accounting.totalValue)
      : null;
  return (
    <section
      className={`investment-overview${cash ? " cash-overview" : ""}`}
      aria-label={cash ? "现金概览" : "投资持仓概览"}
    >
      <div className="investment-overview-grid">
        {metrics.map((key) => {
          const ratioPct =
            !cash &&
            (key === "unrealized" || key === "realized") &&
            totals[key].value != null &&
            totalAssets != null
              ? (Number(totals[key].value) / totalAssets) * 100
              : null;
          return (
            <Card key={key} size="small" className="portfolio-metric-card">
              <article className={"investment-overview-metric metric-" + key}>
                <span>
                  {cash ? "存款与现金" : labels[key]}{" "}
                  <small>{baseCurrency}</small>
                </span>
                <Money
                  as="strong"
                  className={`num ${key === "unrealized" || key === "realized" ? (Number(totals[key].value) === 0 ? "" : Number(totals[key].value) > 0 ? "gain" : "loss") : ""}`}
                >
                  {totals[key].value == null
                    ? "—"
                    : key === "unrealized" || key === "realized"
                      ? formatSigned(totals[key].value!, (value) =>
                          formatMoney(value, baseCurrency),
                        )
                      : formatMoney(totals[key].value!, baseCurrency)}
                </Money>
                {ratioPct != null && (
                  <small className="investment-metric-ratio">
                    占总资产{" "}
                    <span className="num">{formatPercent(ratioPct)}</span>
                  </small>
                )}
                {totals[key].missing > 0 && (
                  <span className="metric-missing">
                    <Badge>待补 {totals[key].missing}</Badge>
                  </span>
                )}
              </article>
            </Card>
          );
        })}
      </div>
    </section>
  );
}
