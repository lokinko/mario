import type { Snapshot } from "../../types";
import { formatMoney } from "../../lib/format";
import { analyzePortfolio } from "./analytics";

export function FinancialSummary({ snapshot }: { snapshot: Snapshot }) {
  const a = analyzePortfolio(snapshot);
  const currency = snapshot.profile.baseCurrency;
  const money = (value: number | null) =>
    value == null ? "待补汇率" : formatMoney(value, currency);
  const pct = (value: number | null) =>
    value == null ? "—" : `${value.toFixed(1)}%`;
  return (
    <>
      <section
        className="metric-grid dashboard-metrics"
        aria-label="资产核心指标"
      >
        <article className="metric hero-metric">
          <span>净资产 · {currency}</span>
          <strong>{money(a.netAssets)}</strong>
          <small>已录入总资产减负债</small>
        </article>
        <article className="metric">
          <span>总资产</span>
          <strong>{money(a.total)}</strong>
          <small>现金与投资持仓合计 · {snapshot.holdings.length} 项</small>
        </article>
        <article className="metric">
          <span>负债总额</span>
          <strong>{money(snapshot.profile.liabilities)}</strong>
          <small>负债 / 总资产 {pct(a.debtRatio)}</small>
        </article>
        <article className="metric">
          <span>存款与现金</span>
          <strong>{money(a.cash)}</strong>
          <small>现金占比 {pct(a.cashPct)} · 含预留备用金</small>
        </article>
        <article className="metric">
          <span>投资资产</span>
          <strong>{money(a.investments)}</strong>
          <small>{a.investmentCount} 项非现金持仓</small>
        </article>
        <article className="metric">
          <span>已录成本持仓浮动盈亏</span>
          <strong className={a.pnl == null ? "" : a.pnl >= 0 ? "gain" : "loss"}>
            {a.pnl == null ? "—" : money(a.pnl)}
          </strong>
          <small>
            {a.costedCount}/{a.investmentCount} 项成本可用 · 成本口径{" "}
            {pct(a.pnlPct)}
          </small>
        </article>
      </section>
      <section
        className="two-columns dashboard-section"
        aria-label="收支与流动性"
      >
        <article className="panel">
          <div className="panel-title">
            <h2>收支与投入能力</h2>
            <span>每月 · {currency}</span>
          </div>
          <dl className="dashboard-facts">
            <div>
              <dt>月收入</dt>
              <dd>{money(snapshot.profile.monthlyIncome)}</dd>
            </div>
            <div>
              <dt>月支出</dt>
              <dd>{money(snapshot.profile.monthlyExpense)}</dd>
            </div>
            <div>
              <dt>月结余 / 储蓄率</dt>
              <dd className={a.surplus < 0 ? "loss" : ""}>
                {money(a.surplus)} / {pct(a.savingsRate)}
              </dd>
            </div>
            <div>
              <dt>目标计划月投入</dt>
              <dd>{money(a.committed)}</dd>
            </div>
            <div>
              <dt>扣除目标投入后月余量</dt>
              <dd className={a.uncommittedMonthly < 0 ? "loss" : "gain"}>
                {money(a.uncommittedMonthly)}
              </dd>
            </div>
          </dl>
          <p className="dashboard-note">
            {a.uncommittedMonthly < 0
              ? "当前目标计划投入超过月结余，需要调整投入或收支。"
              : "余量依据已填写的固定收支计算，未计入临时支出和债务还款。"}
          </p>
        </article>
        <article className="panel">
          <div className="panel-title">
            <h2>备用金与流动性</h2>
            <span>备用金不重复计入资产</span>
          </div>
          <dl className="dashboard-facts">
            <div>
              <dt>预留备用金</dt>
              <dd>{money(snapshot.profile.emergencyFund)}</dd>
            </div>
            <div>
              <dt>应急覆盖</dt>
              <dd>
                {a.emergencyMonths == null
                  ? "待补月支出"
                  : `${a.emergencyMonths.toFixed(1)} 个月`}
              </dd>
            </div>
            <div>
              <dt>现金扣除备用金后余额</dt>
              <dd>{money(a.freeCash)}</dd>
            </div>
            <div>
              <dt>达到 6 个月支出的预留缺口</dt>
              <dd>
                {snapshot.profile.monthlyExpense > 0
                  ? money(
                      Math.max(
                        0,
                        snapshot.profile.monthlyExpense * 6 -
                          snapshot.profile.emergencyFund,
                      ),
                    )
                  : "待补月支出"}
              </dd>
            </div>
          </dl>
          {a.reserveShortfall != null && a.reserveShortfall > 0 && (
            <p className="dashboard-note loss">
              预留备用金比已录入现金多 {money(a.reserveShortfall)}
              ，请核对现金余额或预留金额。
            </p>
          )}
          <p className="dashboard-note">
            6
            个月是应急覆盖参考线。现金余额未扣未来开支及还款，不代表可直接投资的金额。
          </p>
        </article>
      </section>
    </>
  );
}
