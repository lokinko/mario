import { useState } from "react";
import type { Snapshot } from "../../types";
import type { View, FoundationSection } from "../../app/navigation";
import { formatMoney } from "../../lib/format";
import { localDateValue } from "../../lib/dates";
import { analyzePortfolio } from "./analytics";
import { allocationGradient } from "../portfolio/presentation";

export function HoldingAnalysis({
  snapshot,
  navigate,
}: {
  snapshot: Snapshot;
  navigate: (view: View, section?: FoundationSection) => void;
}) {
  const [dimension, setDimension] = useState<"assetClass" | "currency">(
    "assetClass",
  );
  const [sort, setSort] = useState<"value" | "pnl" | "deviation">("value");
  const a = analyzePortfolio(snapshot);
  const currency = snapshot.profile.baseCurrency;
  const allocation = dimension === "assetClass" ? a.byClass : a.byCurrency;
  const today = localDateValue(new Date());
  const rows = [...a.rows].sort((left, right) => {
    const score = (row: typeof left) =>
      sort === "pnl"
        ? row.pnl
        : sort === "deviation"
          ? a.targetsValid && row.weight != null
            ? Math.abs(row.weight - row.holding.targetPct)
            : null
          : row.value;
    return (score(right) ?? -Infinity) - (score(left) ?? -Infinity);
  });
  const money = (value: number | null) =>
    value == null ? "—" : formatMoney(value, currency);
  const pct = (value: number | null) =>
    value == null ? "—" : `${value.toFixed(1)}%`;
  return (
    <section className="panel dashboard-section" aria-label="持仓分析">
      <div className="panel-title">
        <div>
          <h2>资产配置与持仓分析</h2>
          <p className="dashboard-note">所有金额按 {currency} 折算</p>
        </div>
        <button
          className="text-button"
          onClick={() => navigate("foundation", "holdings")}
        >
          维护成本与目标权重 →
        </button>
      </div>
      <div className="dashboard-allocation-grid">
        <div>
          <div className="section-switcher" role="group" aria-label="分布维度">
            <button
              aria-pressed={dimension === "assetClass"}
              onClick={() => setDimension("assetClass")}
            >
              资产类别
            </button>
            <button
              aria-pressed={dimension === "currency"}
              onClick={() => setDimension("currency")}
            >
              持仓币种
            </button>
          </div>
          {a.total != null && a.total > 0 ? (
            <div className="allocation">
              <div
                className="donut"
                aria-hidden="true"
                style={{ background: allocationGradient(allocation) }}
              >
                <div>
                  <strong>{allocation.length}</strong>
                  <span>
                    {dimension === "assetClass" ? "类资产" : "种币种"}
                  </span>
                </div>
              </div>
              <div className="legend">
                {allocation.map((item, index) => (
                  <div key={item.name}>
                    <i className={`color-${index % 5}`} />
                    <span>{item.name}</span>
                    <strong>
                      {pct(item.pct)}
                      <small>{money(item.value)}</small>
                    </strong>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <p className="empty">
              {a.comparable
                ? "添加资产余额后查看分布。"
                : "补齐汇率后查看分布。"}
            </p>
          )}
        </div>
        <dl className="dashboard-facts">
          <div>
            <dt>最大单项占比</dt>
            <dd>
              {pct(
                a.total != null && a.total > 0
                  ? (a.sorted[0]?.weight ?? null)
                  : null,
              )}
            </dd>
          </div>
          <div>
            <dt>前三项资产占比</dt>
            <dd>{pct(a.top3Pct)}</dd>
          </div>
          <div>
            <dt>外币持仓占比</dt>
            <dd>{pct(a.foreignPct)}</dd>
          </div>
          <div>
            <dt>投资市值的成本覆盖率</dt>
            <dd>{pct(a.costCoveragePct)}</dd>
          </div>
          <div>
            <dt>证券价格核验</dt>
            <dd>
              {
                snapshot.holdings.filter((h) =>
                  snapshot.holdingValuations.some((v) => v.holdingId === h.id),
                ).length
              }
              /{snapshot.holdings.length} 项
            </dd>
          </div>
        </dl>
      </div>
      <div className="dashboard-table-heading">
        <h3>逐项持仓</h3>
        <label>
          排序{" "}
          <select
            aria-label="持仓排序"
            value={sort}
            onChange={(event) => setSort(event.target.value as typeof sort)}
          >
            <option value="value">市值从高到低</option>
            <option value="pnl">浮盈从高到低</option>
            <option value="deviation">配置偏差从大到小</option>
          </select>
        </label>
      </div>
      {!a.rows.length ? (
        <p className="empty">还没有持仓。先在“我的情况”填写存款和投资余额。</p>
      ) : (
        <div
          className="dashboard-table-scroll"
          tabIndex={0}
          role="region"
          aria-label="持仓明细，可横向滚动"
        >
          <table className="dashboard-table">
            <thead>
              <tr>
                <th>资产 / 类别</th>
                <th>市值 / 占比</th>
                <th>累计成本</th>
                <th>浮动盈亏 / 成本口径</th>
                <th>目标 / 偏差</th>
                <th>估值日期 / 来源</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.holding.id}>
                  <td>
                    <strong>{row.holding.name}</strong>
                    <small>
                      {row.holding.assetClass} · {row.holding.currency}
                      {row.holding.symbol ? ` · ${row.holding.symbol}` : ""}
                    </small>
                  </td>
                  <td>
                    {money(row.value)}
                    <small>{pct(row.weight)}</small>
                  </td>
                  <td>
                    {row.holding.assetClass === "现金"
                      ? "不适用"
                      : row.cost == null
                        ? "待补成本"
                        : money(row.cost)}
                  </td>
                  <td
                    className={
                      row.pnl == null ? "" : row.pnl >= 0 ? "gain" : "loss"
                    }
                  >
                    {money(row.pnl)}
                    <small>{pct(row.pnlPct)}</small>
                  </td>
                  <td>
                    {a.targetsValid
                      ? `${row.holding.targetPct.toFixed(1)}%`
                      : "待设目标"}
                    <small>
                      {a.targetsValid && row.weight != null
                        ? `${row.weight - row.holding.targetPct >= 0 ? "+" : ""}${(row.weight - row.holding.targetPct).toFixed(1)} 个百分点`
                        : "—"}
                    </small>
                  </td>
                  <td>
                    {row.holding.valuationDate || "待补日期"}
                    <small>
                      {snapshot.holdingValuations.some(
                        (v) => v.holdingId === row.holding.id,
                      )
                        ? "来源价格已核验"
                        : "用户填写"}
                      {row.holding.valuationDate &&
                      row.holding.valuationDate < today
                        ? " · 非今日估值"
                        : ""}
                    </small>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="dashboard-note">
        浮动盈亏只汇总非现金且已录成本的持仓；成本为 0
        按未录入处理。市值和成本都按当前汇率折算，未分离汇兑损益，未计入已实现盈亏、分红及税费，不等于总投资收益。
      </p>
      {!a.targetsValid && a.rows.length > 0 && (
        <p className="dashboard-note">
          目标权重合计 {a.targetTotal.toFixed(1)}%，补齐至 100%
          后显示配置偏差。币种分布依据记账币种，不能代表基金底层资产的真实外汇敞口。
        </p>
      )}
    </section>
  );
}
