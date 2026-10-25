import { Callout } from "../../components/Callout";
import { useRef, useState } from "react";
import { Badge } from "../../components/Badge";
import type { Holding, Snapshot } from "../../types";
import { formatMoney, formatAmount, formatSigned } from "../../lib/format";
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
  return (
    <section
      className="investment-overview"
      aria-label={cash ? "现金概览" : "投资持仓概览"}
    >
      <div className="investment-overview-grid">
        {metrics.map((key) => (
          <article
            key={key}
            className={"investment-overview-metric metric-" + key}
          >
            <span>
              {cash ? "存款与现金" : labels[key]} <small>{baseCurrency}</small>
            </span>
            <strong
              className={`num ${key === "unrealized" || key === "realized" ? (Number(totals[key].value) === 0 ? "" : Number(totals[key].value) > 0 ? "gain" : "loss") : ""}`}
              data-money
            >
              {totals[key].value == null
                ? "—"
                : key === "unrealized" || key === "realized"
                  ? formatSigned(totals[key].value!, (value) =>
                      formatMoney(value, baseCurrency),
                    )
                  : formatMoney(totals[key].value!, baseCurrency)}
            </strong>
            {totals[key].missing > 0 && (
              <button
                className="metric-missing"
                onClick={() => {
                  setExpanded(true);
                  requestAnimationFrame(() => {
                    const row = breakdown.current?.querySelector<HTMLElement>(
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
            <small
              title={
                totals[key].missing
                  ? `${totals[key].missing} 项成本、交易或汇率待补；缺失数据不计入已知部分`
                  : undefined
              }
            >
              {totals[key].missing
                ? "已知部分 · " +
                  totals[key].missing +
                  " 项" +
                  (key === "realized"
                    ? "交易或汇率"
                    : key === "value"
                      ? "汇率"
                      : "成本或汇率") +
                  "待补"
                : key === "realized"
                  ? "按当前汇率折算 · 含已清仓资产与净现金分红"
                  : "全部" + (cash ? "币种余额" : "投资持仓")}
            </small>
          </article>
        ))}
      </div>
      <div className="overview-breakdown">
        <div className="overview-caption">
          按持仓估值汇率折算 {baseCurrency} · {cash ? "币种余额" : "不含现金"}{" "}
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
            <p>
              成本与收益按各持仓估值汇率折算，不代表含历史汇兑损益的收益。缺失数据不计入已知部分。
            </p>
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
                        <td key={key} className="num" data-money>
                          {amounts[key] == null
                            ? "—"
                            : formatAmount(amounts[key]!, holding.currency)}
                        </td>
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
