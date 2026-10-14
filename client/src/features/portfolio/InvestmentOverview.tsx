import type { Holding, Snapshot } from "../../types";
import { formatMoney } from "../../lib/format";
import { portfolioOverview, overviewMetrics } from "./portfolioOverview";
const labels = {
  value: "持仓市值",
  cost: "持仓成本",
  unrealized: "浮动盈亏",
  realized: "已结算盈亏",
};
export function InvestmentOverview({
  holdings,
  summaries,
  baseCurrency = "CNY",
  cash = false,
}: {
  holdings: Holding[];
  summaries: Snapshot["holdingTradeSummaries"];
  baseCurrency?: string;
  cash?: boolean;
}) {
  const { totals, rows } = portfolioOverview(holdings, summaries, baseCurrency);
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
              className={
                key === "unrealized" || key === "realized"
                  ? totals[key].value == null
                    ? ""
                    : totals[key].value >= 0
                      ? "gain"
                      : "loss"
                  : ""
              }
            >
              {totals[key].value == null
                ? "—"
                : ((key === "unrealized" || key === "realized") &&
                  totals[key].value! > 0
                    ? "+"
                    : "") + formatMoney(totals[key].value!, baseCurrency)}
            </strong>
            <small>
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
                  ? "含已清仓资产与净现金分红"
                  : "全部" + (cash ? "现金账户" : "投资持仓")}
            </small>
          </article>
        ))}
      </div>
      <details className="overview-breakdown">
        <summary>
          按持仓估值汇率折算 {baseCurrency} · {cash ? "现金账户" : "不含现金"}{" "}
          <span>原币明细</span>
        </summary>
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
                <tr key={holding.id}>
                  <th>
                    {holding.name}
                    <small>{holding.currency}</small>
                  </th>
                  {metrics.map((key) => (
                    <td key={key}>
                      {amounts[key] == null
                        ? "—"
                        : formatMoney(amounts[key]!, holding.currency)}
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
      </details>
    </section>
  );
}
