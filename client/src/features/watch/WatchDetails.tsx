import type { WatchRow } from "../../api/watch";
import type { Snapshot } from "../../types";
import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { BandTrack, RuleChecks, YieldGrid } from "./WatchSignals";
import { canonical, number, statuses } from "./presentation";

export function WatchDetails({
  row,
  snapshot,
  minYield,
  tolerance,
  busy,
  error,
  onClose,
  onAsk,
  onEdit,
  onRemove,
}: {
  row: WatchRow;
  snapshot: Snapshot;
  minYield: number;
  tolerance: number;
  busy: boolean;
  error: string;
  onClose: () => void;
  onAsk: () => void;
  onEdit: () => void;
  onRemove: () => void;
}) {
  const { entry, latest } = row;
  const related = snapshot.holdings.filter(
    (h) => canonical(h.symbol) === entry.symbol,
  );
  const ids = new Set(related.map((h) => h.id));
  const trades = (snapshot.holdingTrades ?? [])
    .filter((t) => ids.has(t.holdingId))
    .sort(
      (a, b) =>
        b.occurredOn.localeCompare(a.occurredOn) || b.sequence - a.sequence,
    )
    .slice(0, 5);
  return (
    <Drawer
      title={entry.name}
      subtitle={`${entry.symbol} · ${latest ? statuses[latest.status] : "等待首次检查"}`}
      onClose={onClose}
      busy={busy}
      size="wide"
    >
      <div className="watch-detail-body">
        {error && <Callout tone="danger">{error}</Callout>}
        <div className="watch-metrics">
          <div>
            <span>收盘价 · {latest?.observedOn ?? "—"}</span>
            <strong>{number(latest?.price)}</strong>
          </div>
          <div>
            <span>TTM 税前股息率</span>
            <strong>{number(latest?.yieldPct, "%")}</strong>
          </div>
          <div>
            <span>TTM 每股分红 / 元</span>
            <strong>{latest?.dividendPerShare ?? "—"}</strong>
          </div>
        </div>
        <RuleChecks row={row} minYield={minYield} />
        <div className="watch-bands">
          <BandTrack
            label="日线"
            band={latest?.daily}
            price={latest?.price}
            tolerance={tolerance}
          />
          <BandTrack
            label="周线"
            band={latest?.weekly}
            price={latest?.price}
            tolerance={tolerance}
          />
        </div>
        <YieldGrid row={row} />
        <section className="watch-detail-section">
          <h3>分红依据</h3>
          {latest?.dividendBasis && (
            <p className="watch-meta">
              分红资料覆盖至 {latest.dividendBasis.asOf} · 采集于{" "}
              {new Date(latest.dividendBasis.fetchedAt).toLocaleString(
                "zh-CN",
                { timeZone: "Asia/Shanghai" },
              )}
              。股息率按上述行情日期的收盘价重算。
            </p>
          )}
          {latest?.dividendEvents.length ? (
            <div className="table-scroll">
              <table className="watch-events">
                <caption>本次 TTM 窗口的已实施事件（每十股，税前）</caption>
                <thead>
                  <tr>
                    <th>除息日</th>
                    <th>报告期</th>
                    <th>派现 / 元</th>
                    <th>送股</th>
                    <th>转增</th>
                  </tr>
                </thead>
                <tbody>
                  {[...latest.dividendEvents]
                    .sort((a, b) => b.exDate.localeCompare(a.exDate))
                    .map((e, i) => (
                      <tr key={`${e.exDate}-${e.reportDate ?? i}`}>
                        <td>除息日 {e.exDate}</td>
                        <td>{e.reportDate ? `报告期 ${e.reportDate}` : "—"}</td>
                        <td>{e.cashPerTen}</td>
                        <td>{e.bonusPerTen}</td>
                        <td>{e.transferPerTen}</td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
          ) : (
            <p className="watch-meta">
              {latest?.dividendPerShare === "0"
                ? "本次窗口内已核验的现金分红为零。"
                : "本次观察暂无可展示的分红事件。"}
            </p>
          )}
          <div className="watch-sources">
            {latest?.sources.map((url, i) => (
              <a key={url} href={url} target="_blank" rel="noreferrer">
                {i === 0
                  ? "证券日线来源"
                  : i === 1
                    ? "交易日期参考来源"
                    : "已实施分红来源"}{" "}
                ↗
              </a>
            ))}
          </div>
        </section>
        <section className="watch-detail-section">
          <h3>关注理由</h3>
          <p className="watch-thesis">{entry.thesis || "尚未补充关注理由。"}</p>
        </section>
        <section className="watch-detail-section">
          <h3>已有持仓与最近交易</h3>
          {related.map((h) => (
            <p key={h.id}>
              {h.name} · 数量{" "}
              {snapshot.accounting?.holdings[h.id]?.quantity ??
                h.quantity ??
                "—"}{" "}
              · 成本 {snapshot.accounting?.holdings[h.id]?.costBasis ?? "—"}{" "}
              {h.currency}
            </p>
          ))}
          {!trades.length ? (
            <p>没有关联的已录入交易。</p>
          ) : (
            <ul className="watch-trades">
              {trades.map((t) => (
                <li key={t.id}>
                  {t.occurredOn} · {t.account} ·{" "}
                  {
                    {
                      buy: "买入",
                      sell: "卖出",
                      dividend: "分红",
                      expire: "到期",
                    }[t.kind]
                  }{" "}
                  · 成交金额 {t.grossAmount} {t.currency}
                  {t.kind === "buy" || t.kind === "sell"
                    ? ` · 原始成交价 ${t.price}`
                    : ""}
                </li>
              ))}
            </ul>
          )}
          <p className="watch-meta">
            以上仅包含已录入记录；股票拆合股前后的原始成交价不能直接比较。讨论时按问答预设授权发送持仓、交易与研究资料。
          </p>
        </section>
        <div className="watch-actions">
          <button className="primary" onClick={onAsk}>
            结合我的交易讨论
          </button>
          <button className="secondary" disabled={busy} onClick={onEdit}>
            编辑观察资料
          </button>
          <button
            className="text-button danger"
            disabled={busy}
            onClick={onRemove}
          >
            移出观察名单
          </button>
        </div>
        <p className="watch-meta">
          最近检查{" "}
          {row.checkedAt
            ? new Date(row.checkedAt).toLocaleString("zh-CN", {
                timeZone: "Asia/Shanghai",
              })
            : "—"}
        </p>
      </div>
    </Drawer>
  );
}
