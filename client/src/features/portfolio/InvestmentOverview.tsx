import { Money } from "../../components/AmountPrivacy";
import { Callout } from "../../components/Callout";
import { Card } from "antd";
import { useRef, useState } from "react";
import { Badge } from "../../components/Badge";
import type { Holding, Snapshot } from "../../types/user";
import {
  formatMoney,
  formatAmount,
  formatPercent,
  formatSigned,
} from "../../lib/format";
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
  const [expanded, setExpanded] = useState(false);
  const breakdown = useRef<HTMLDivElement>(null);
  if (!accounting)
    return (
      <section aria-label={cash ? "现金概览" : "投资持仓概览"}>
        <Callout tone="danger">缺少精确会计数据，请刷新页面。</Callout>
      </section>
    );
  const { totals, rows } = portfolioOverview(holdings, accounting, cash);
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
                  <button
                    className="metric-missing"
                    onClick={() => {
                      setExpanded(true);
                      requestAnimationFrame(() => {
                        const row =
                          breakdown.current?.querySelector<HTMLElement>(
                            '[data-missing="true"]',
                          );
                        row?.focus();
                        row?.scrollIntoView?.({ block: "nearest" });
                      });
                    }}
                  >
                    <Badge>待补 {totals[key].missing}</Badge>
                  </button>
                )}
              </article>
            </Card>
          );
        })}
      </div>
      <div className="overview-breakdown">
        <div className="overview-caption">
          <button
            className="text-button"
            aria-expanded={expanded}
            onClick={() => setExpanded(!expanded)}
          >
            原币明细 ›
          </button>
        </div>
        {expanded && (
          <div ref={breakdown}>
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>资产 / 原币</th>
                    {metrics.map((key) => (
                      <th key={key}>{labels[key]}</th>
                    ))}
                    <th>汇率 / 日期</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map(({ holding, amounts, rate }) => (
                    <tr
                      key={holding.id}
                      tabIndex={-1}
                      data-missing={
                        Object.values(amounts).some((value) => value == null) ||
                        rate == null
                      }
                    >
                      <th>
                        {holding.name}
                        <small>{holding.currency}</small>
                      </th>
                      {metrics.map((key) => (
                        <Money as="td" key={key} className="num">
                          {amounts[key] == null
                            ? "—"
                            : formatAmount(amounts[key]!, holding.currency)}
                        </Money>
                      ))}
                      <td>
                        {rate ?? "待补"}
                        <small>
                          {holding.currency === baseCurrency
                            ? "基础币种"
                            : holding.fxRateObservedOn || "日期待补"}
                        </small>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
