import { useEffect, useState } from "react";
import { Bell, Plus, RefreshCw, MessageCircle } from "lucide-react";
import {
  getWatchBoard,
  saveWatch,
  deleteWatch,
  readWatchAlerts,
  refreshWatch,
  type WatchBoard,
  type WatchInput,
  type WatchRow,
  type WatchIssue,
} from "../../api/watch";
import type { Snapshot } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { Badge } from "../../components/Badge";
import { Callout } from "../../components/Callout";
import { Drawer } from "../../components/Drawer";
import { useRequestGuard } from "../../lib/useRequestGuard";
import "./watch.css";

const statuses = {
  attention: "进入关注区间",
  waiting: "继续等待",
  incomplete: "待补齐数据",
};
const issueLabels: Record<WatchIssue, string> = {
  price_unavailable: "该交易日收盘价缺失",
  daily_history_incomplete: "日线历史不足或存在缺口",
  weekly_history_incomplete: "周线历史不足或存在缺口",
  dividend_unavailable: "已实施分红数据暂不可用",
  non_positive_band: "下轨不为正，无法使用比例区间",
};
const number = (value: number | null | undefined, suffix = "") =>
  value == null ? "—" : `${value.toFixed(2)}${suffix}`;
const canonical = (value: string) =>
  value.trim().toUpperCase().replace(/\.SH$/, ".SS");
const empty: WatchInput = { symbol: "", name: "", thesis: "" };

export function BuyWatch({
  snapshot,
  onAsk,
  flash,
  onEdited,
  onSaved,
}: {
  snapshot: Snapshot;
  onAsk: (question: string) => void;
  flash: (message: string) => void;
  onEdited: () => void;
  onSaved: () => void;
}) {
  const [board, setBoard] = useState<WatchBoard | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [form, setForm] = useState<WatchInput>(empty);
  const [editing, setEditing] = useState(false);
  const [open, setOpen] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [expanded, setExpanded] = useState<string | null>(null);
  const guard = useRequestGuard();
  const load = async () => {
    const current = guard.begin();
    try {
      const data = await getWatchBoard();
      if (current()) {
        setBoard(data);
        setError("");
      }
    } catch (e) {
      if (current()) setError(String(e));
    }
  };
  useEffect(() => {
    void load();
  }, []);
  const run = async (action: () => Promise<WatchBoard>) => {
    if (busy) return;
    const current = guard.begin();
    setBusy(true);
    setError("");
    try {
      const next = await action();
      if (current()) setBoard(next);
    } catch (e) {
      if (current()) setError(String(e));
    } finally {
      if (current()) setBusy(false);
    }
  };
  const close = () => {
    if (dirty && !window.confirm("放弃未保存的观察资料？")) return;
    setOpen(false);
    setDirty(false);
    onSaved();
  };
  const edit = (value?: WatchInput) => {
    setForm(value ?? empty);
    setEditing(Boolean(value));
    setDirty(false);
    setOpen(true);
  };
  const change = (key: keyof WatchInput, value: string) => {
    setForm({ ...form, [key]: value });
    setDirty(true);
    onEdited();
  };
  const save = () =>
    void run(async () => {
      await saveWatch(form);
      setOpen(false);
      setDirty(false);
      onSaved();
      flash("已保存观察公司，交易日收盘后自动检查，也可现在检查。");
      return getWatchBoard();
    });
  const ask = (row: WatchRow) => {
    const { entry, latest } = row;
    onAsk(
      `请讨论买点观察名单中的${entry.name}（${entry.symbol}）。我的关注理由：${entry.thesis || "尚未补充，请帮助核查"}。观察规则是已实施税前现金分红TTM股息率至少5%，日线和周线BOLL(20,2)收盘价都在各自下轨价格的上下2%内。${latest ? `最近观察日期是${latest.observedOn}，状态为${statuses[latest.status]}。` : "尚无行情观察结果。"}请结合授权的当前持仓、现金和最近真实交易，讨论现在是否值得进一步研究买入或加仓；核查分红持续性和公司经营的支持与反方证据，区分已知事实和待核查项。不要仅由BOLL位置断言低估，也不要给出未经约定的精确买入数量。`,
    );
  };
  const rows = board?.rows ?? [];
  const holdings = snapshot.holdings.filter((h) =>
    /^(60|68)\d{4}\.(SS|SH)$|^(00|30)\d{4}\.SZ$/i.test(h.symbol),
  );
  const counts = {
    attention: rows.filter((r) => r.latest?.status === "attention").length,
    alerts: rows.filter((r) => r.alert).length,
  };
  const week = board?.weekly ?? [];
  const weeklyAsk = () => {
    const selected = rows.slice(0, 5);
    onAsk(
      `请为买点观察名单做${board?.weekStart}至${board?.weekEnd}的周末复盘，先讨论${selected.map((r) => `${r.entry.name}（${r.entry.symbol}，关注理由：${r.entry.thesis || "待补充"}）`).join("、")}。本周已保存${week.length}条收盘观察。请按5%股息率、日周BOLL下轨±2%的规则，结合授权的观察资料、持仓和最近交易，说明下周应该等待什么，以及分红和经营判断需要核查哪些证据。未提供历史时不要推测周内变化；不预设必须买入，不输出精确买入数量。`,
    );
  };
  return (
    <div className="page watch-page">
      <PageHeader
        title="买点观察"
        description="照看你认可的公司，等待值得研究的变化。"
      />
      <section className="panel watch-intro">
        <div>
          <strong>股息率 ≥ 5%</strong>
          <span>日线、周线均在 BOLL 下轨 ±2%</span>
        </div>
        <p>
          北京时间 19:00
          后检查当日收盘数据，站内保留变化提醒；周末集中复盘。基本面与分红持续性在讨论中核查。
        </p>
        <div className="watch-actions">
          <button className="primary" disabled={busy} onClick={() => edit()}>
            <Plus size={16} />
            加入公司
          </button>
          <button
            className="secondary"
            disabled={busy || !rows.length}
            onClick={() => void run(refreshWatch)}
          >
            <RefreshCw size={16} />
            {busy ? "处理中…" : "检查最新收盘"}
          </button>
        </div>
      </section>
      {error && !open && (
        <Callout tone="danger">
          <span>{error}</span>
          <button disabled={busy} onClick={() => void load()}>
            重新读取
          </button>
        </Callout>
      )}
      {!board && !error && <p role="status">正在读取观察名单…</p>}
      {board && (
        <>
          <div className="watch-summary">
            <span>{rows.length} 家观察公司</span>
            <span>{counts.attention} 家进入关注区间</span>
            <span>
              <Bell size={15} />
              {counts.alerts} 家有未读变化
            </span>
          </div>
          {!rows.length && (
            <section className="panel">
              <h3>从一家公司开始</h3>
              <p>
                加入你愿意长期研究的公司；已有持仓可以直接选择，未持有的公司也能观察。
              </p>
              <button className="secondary" onClick={() => edit()}>
                加入第一家公司
              </button>
            </section>
          )}
          <div className="watch-list">
            {rows.map((row) => {
              const { entry, latest } = row;
              const related = snapshot.holdings.filter(
                (h) => canonical(h.symbol) === entry.symbol,
              );
              const ids = new Set(related.map((h) => h.id));
              const trades = (snapshot.holdingTrades ?? [])
                .filter((t) => ids.has(t.holdingId))
                .sort(
                  (a, b) =>
                    b.occurredOn.localeCompare(a.occurredOn) ||
                    b.sequence - a.sequence,
                )
                .slice(0, 5);
              return (
                <article
                  className={`panel watch-card ${row.alert ? "watch-unread" : ""}`}
                  key={entry.symbol}
                >
                  <div className="watch-heading">
                    <div>
                      <h3>
                        {entry.name}
                        <small>{entry.symbol}</small>
                      </h3>
                      <span>
                        {related.length
                          ? `关联 ${related.length} 个账户持仓`
                          : "尚未持有"}
                      </span>
                    </div>
                    <Badge
                      tone={
                        latest?.status === "attention" ? "success" : "neutral"
                      }
                    >
                      {latest ? statuses[latest.status] : "等待首次检查"}
                    </Badge>
                  </div>
                  {row.alert && (
                    <p className="watch-change">
                      <Bell size={15} />
                      关注状态发生变化
                      <button
                        className="text-button"
                        disabled={busy}
                        onClick={() =>
                          void run(async () => {
                            await readWatchAlerts(entry.symbol);
                            return getWatchBoard();
                          })
                        }
                      >
                        标为已读
                      </button>
                    </p>
                  )}
                  <div className="watch-metrics">
                    <div>
                      <span>收盘价</span>
                      <strong>{number(latest?.price)}</strong>
                    </div>
                    <div>
                      <span>TTM 税前股息率</span>
                      <strong>{number(latest?.yieldPct, "%")}</strong>
                    </div>
                    <div>
                      <span>距日线下轨</span>
                      <strong>{number(latest?.daily?.distancePct, "%")}</strong>
                    </div>
                    <div>
                      <span>距周线下轨</span>
                      <strong>
                        {number(latest?.weekly?.distancePct, "%")}
                      </strong>
                    </div>
                  </div>
                  <p className="watch-meta">
                    行情日期 {latest?.observedOn ?? "—"} · 最近检查{" "}
                    {row.checkedAt
                      ? new Date(row.checkedAt).toLocaleString("zh-CN", {
                          timeZone: "Asia/Shanghai",
                        })
                      : "—"}
                  </p>
                  {(row.refreshFailed || Boolean(latest?.issues.length)) && (
                    <div className="watch-data-issue">
                      <span>
                        {row.refreshFailed
                          ? "本次来源读取失败，保留上次结果。"
                          : latest?.issues
                              .map((i) => issueLabels[i])
                              .join("；")}
                      </span>
                      <button
                        className="text-button"
                        disabled={busy}
                        onClick={() => void run(refreshWatch)}
                      >
                        重新检查数据
                      </button>
                    </div>
                  )}
                  {entry.thesis && (
                    <p className="watch-thesis">关注理由：{entry.thesis}</p>
                  )}
                  <div className="watch-actions">
                    <button className="primary" onClick={() => ask(row)}>
                      <MessageCircle size={16} />
                      结合我的交易讨论
                    </button>
                    <button
                      className="text-button"
                      onClick={() =>
                        setExpanded(
                          expanded === entry.symbol ? null : entry.symbol,
                        )
                      }
                    >
                      {expanded === entry.symbol
                        ? "收起依据"
                        : "查看依据与交易"}
                    </button>
                    <button
                      className="text-button"
                      disabled={busy}
                      onClick={() => edit(entry)}
                    >
                      编辑
                    </button>
                  </div>
                  {expanded === entry.symbol && (
                    <div className="watch-details">
                      <p>
                        BOLL 使用 20 期收盘均值 ±2
                        倍总体标准差。日线和周线分别计算；本周尚未结束时，周线使用截至观察日的收盘。距下轨
                        =（收盘价 ÷ 下轨 − 1）×100%。
                      </p>
                      <p>
                        技术价格使用供应商调整收盘价，并归一到当前价格单位。TTM
                        股息采用过去 365
                        天已实施的税前现金分红（含特别分红），按送转股份调整单位；未来分红需另行研究。
                      </p>
                      <p>
                        日线下轨 {number(latest?.daily?.lower)} · 周线下轨{" "}
                        {number(latest?.weekly?.lower)} · TTM 每股分红{" "}
                        {latest?.dividendPerShare ?? "—"} 元
                      </p>
                      <ul>
                        {latest?.sources.map((url, i) => (
                          <li key={url}>
                            <a href={url} target="_blank" rel="noreferrer">
                              {i === 0
                                ? "证券日线来源"
                                : i === 1
                                  ? "交易日期参考来源"
                                  : "已实施分红来源"}
                            </a>
                          </li>
                        ))}
                      </ul>
                      <h4>已有持仓与最近交易</h4>
                      {related.map((h) => (
                        <p key={h.id}>
                          {h.name} · 数量{" "}
                          {snapshot.accounting?.holdings[h.id]?.quantity ??
                            h.quantity ??
                            "—"}{" "}
                          · 成本{" "}
                          {snapshot.accounting?.holdings[h.id]?.costBasis ??
                            "—"}{" "}
                          {h.currency}
                        </p>
                      ))}
                      {!trades.length ? (
                        <p>没有关联的已录入交易。</p>
                      ) : (
                        <ul>
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
                      <p>
                        以上仅包含已录入记录；股票拆合股前后的原始成交价不能直接比较。讨论时按问答预设授权发送持仓、交易与研究资料。
                      </p>
                      <button
                        className="text-button danger"
                        disabled={busy}
                        onClick={() => {
                          if (
                            window.confirm(
                              `移除 ${entry.name} 的观察资料与观察历史？持仓和交易记录保留。`,
                            )
                          )
                            void run(async () => {
                              await deleteWatch(entry.symbol);
                              return getWatchBoard();
                            });
                        }}
                      >
                        移出观察名单
                      </button>
                    </div>
                  )}
                </article>
              );
            })}
          </div>
          <section className="panel watch-weekly">
            <h3>
              本周复盘{" "}
              <small>
                {board.weekStart} — {board.weekEnd}
              </small>
            </h3>
            <p>
              本周已保存 {week.length} 条收盘观察，
              {
                new Set(
                  week
                    .filter((v) => v.status === "attention")
                    .map((v) => v.symbol),
                ).size
              }{" "}
              家公司曾进入关注区间。空白日期表示没有保存的检查结果。
            </p>
            {rows.map((row) => {
              const records = week.filter((v) => v.symbol === row.entry.symbol);
              return (
                <div className="watch-week-row" key={row.entry.symbol}>
                  <strong>{row.entry.name}</strong>
                  <span>
                    {records.length
                      ? records
                          .map(
                            (v) =>
                              `${v.observedOn.slice(5)} ${statuses[v.status]}`,
                          )
                          .join("；")
                      : "本周暂无观察记录"}
                  </span>
                </div>
              );
            })}
            <button
              className="secondary"
              disabled={!rows.length}
              onClick={weeklyAsk}
            >
              结合持仓做周末复盘{rows.length > 5 ? "（先讨论前 5 家）" : ""}
            </button>
          </section>
        </>
      )}
      <>
        {open && (
          <Drawer
            busy={busy}
            dirty={dirty}
            onClose={() => {
              setOpen(false);
              setDirty(false);
              onSaved();
            }}
            title={editing ? "编辑观察公司" : "加入观察公司"}
          >
            <form
              onSubmit={(e) => {
                e.preventDefault();
                save();
              }}
            >
              {error && <Callout tone="danger">{error}</Callout>}
              {!editing && holdings.length > 0 && (
                <label>
                  从已有持仓选择
                  <select
                    aria-label="从已有持仓选择"
                    value=""
                    onChange={(e) => {
                      const h = holdings.find((h) => h.id === e.target.value);
                      if (h) {
                        setForm({
                          ...form,
                          symbol: canonical(h.symbol),
                          name: h.name,
                        });
                        setDirty(true);
                        onEdited();
                      }
                    }}
                  >
                    <option value="">手动填写或选择持仓</option>
                    {holdings.map((h) => (
                      <option key={h.id} value={h.id}>
                        {h.name} · {h.symbol}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              <div className="form-grid">
                <label>
                  证券代码
                  <input
                    required
                    disabled={editing || busy}
                    value={form.symbol}
                    placeholder="600036.SS"
                    onChange={(e) => change("symbol", e.target.value)}
                  />
                </label>
                <label>
                  公司名称
                  <input
                    required
                    maxLength={80}
                    disabled={busy}
                    value={form.name}
                    onChange={(e) => change("name", e.target.value)}
                  />
                </label>
              </div>
              <label>
                我认可它的理由
                <textarea
                  rows={5}
                  maxLength={2000}
                  disabled={busy}
                  value={form.thesis}
                  placeholder="例如关注的业务、分红依据，以及需要持续核查的事情。可稍后补充。"
                  onChange={(e) => change("thesis", e.target.value)}
                />
              </label>
              <p>
                保存后纳入自动收盘观察。你填写的理由是研究起点，基本面是否稳定需要持续核查。
              </p>
              <div className="form-actions">
                <button
                  type="button"
                  className="secondary"
                  disabled={busy}
                  onClick={close}
                >
                  取消
                </button>
                <button
                  type="submit"
                  className="primary"
                  disabled={busy || !form.symbol.trim() || !form.name.trim()}
                >
                  保存观察公司
                </button>
              </div>
            </form>
          </Drawer>
        )}
      </>
    </div>
  );
}
