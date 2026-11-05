import { useEffect, useState } from "react";
import { Button, Input, Tag } from "antd";
import {
  Plus,
  RefreshCw,
  Search,
  List,
  LayoutGrid,
  SlidersHorizontal,
  ChartNoAxesCombined,
  ArrowUpRight,
} from "lucide-react";
import {
  getWatchBoard,
  saveWatch,
  deleteWatch,
  readWatchAlerts,
  refreshWatch,
  type WatchBoard,
  type WatchInput,
  type WatchRow,
} from "../../api/watch";
import type { Snapshot } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { Callout } from "../../components/Callout";
import { Drawer } from "../../components/Drawer";
import { useRequestGuard } from "../../lib/useRequestGuard";
import "./watch.css";

import { WatchList } from "./WatchList";
import { WatchDetails } from "./WatchDetails";
import { WatchWeekly } from "./WatchWeekly";
import {
  canonical,
  statuses,
  selectRows,
  checks,
  type WatchFilter,
  type WatchSort,
} from "./presentation";

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
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<WatchFilter>("all");
  const [sort, setSort] = useState<WatchSort>("default");
  const [ownership, setOwnership] = useState("all");
  const [view, setView] = useState<"compact" | "cards">("compact");
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
      `请讨论买点观察名单中的${entry.name}（${entry.symbol}）。我的关注理由：${entry.thesis || "尚未补充，请帮助核查"}。观察规则是已实施税前现金分红TTM股息率至少5%，日线BOLL(20,2)收盘价在日下轨价格的上下2%内、周收盘价≤周线BOLL(20,2)中轨。${latest ? `最近观察日期是${latest.observedOn}，状态为${statuses[latest.status]}。` : "尚无行情观察结果。"}请结合授权的当前持仓、现金和最近真实交易，讨论现在是否值得进一步研究买入或加仓；核查分红持续性和公司经营的支持与反方证据，区分已知事实和待核查项。不要仅由BOLL位置断言低估，也不要给出未经约定的精确买入数量。`,
    );
  };
  const rows = board?.rows ?? [];
  const holdings = snapshot.holdings.filter((h) =>
    /^(60|68)\d{4}\.(SS|SH)$|^(00|30)\d{4}\.SZ$/i.test(h.symbol),
  );
  const minYield = board?.minYieldPct ?? 5;
  const tolerance = board?.bandDistancePct ?? 2;
  const held = new Set(
    holdings
      .filter(
        (h) =>
          Number(
            snapshot.accounting?.holdings[h.id]?.quantity ?? h.quantity ?? "0",
          ) > 0,
      )
      .map((h) => canonical(h.symbol)),
  );
  const counts = {
    attention: rows.filter((r) => r.latest?.status === "attention").length,
    yield: rows.filter((r) => checks(r, minYield)[0] === true).length,
    bands: rows.filter((r) =>
      checks(r, minYield)
        .slice(1)
        .every((v) => v === true),
    ).length,
    alerts: rows.filter((r) => r.alert).length,
    incomplete: rows.filter(
      (r) => !r.latest || r.latest.status === "incomplete" || r.refreshFailed,
    ).length,
  };
  const visible = selectRows(
    rows,
    query,
    filter,
    sort,
    held,
    ownership,
    minYield,
  );
  const active = rows.find((r) => r.entry.symbol === expanded);
  const reset = () => {
    setQuery("");
    setFilter("all");
    setOwnership("all");
    setSort("default");
  };
  const week = board?.weekly ?? [];
  const weeklyAsk = () => {
    const selected = rows.slice(0, 5);
    onAsk(
      `请为买点观察名单做${board?.weekStart}至${board?.weekEnd}的周末复盘，先讨论${selected.map((r) => `${r.entry.name}（${r.entry.symbol}，关注理由：${r.entry.thesis || "待补充"}）`).join("、")}。本周已保存${week.length}条收盘观察。请按5%股息率、日线BOLL下轨±2%、周收盘价≤周线BOLL中轨的规则，结合授权的观察资料、持仓和最近交易，说明下周应该等待什么，以及分红和经营判断需要核查哪些证据。未提供历史时不要推测周内变化；不预设必须买入，不输出精确买入数量。`,
    );
  };
  return (
    <div className="page watch-page">
      <PageHeader
        title="买点观察"
        description="照看你认可的公司，等待值得研究的变化。"
        action={
          <>
            <Button
              disabled={busy || !rows.length}
              onClick={() => void run(refreshWatch)}
              icon={
                <RefreshCw size={15} className={busy ? "watch-spinning" : ""} />
              }
            >
              {busy ? "处理中…" : "检查最新收盘"}
            </Button>
            <Button
              type="primary"
              disabled={busy}
              onClick={() => edit()}
              icon={<Plus size={16} />}
            >
              加入公司
            </Button>
          </>
        }
      />
      <section className="panel watch-intro">
        <div>
          <div className="watch-intro-heading">
            <span className="watch-intro-icon">
              <ChartNoAxesCombined size={23} />
            </span>
            <div>
              <span className="watch-eyebrow">收盘观察 · 已实施分红</span>
              <strong>有依据地观察，有耐心地等待。</strong>
            </div>
          </div>
          <Tag variant="filled">日线 + 周线 · 双重观察</Tag>
        </div>
        <div className="watch-rule-strip">
          <span>① TTM 税前股息 ≥{minYield}%</span>
          <span>② 日线下轨 ±{tolerance}%</span>
          <span>③ 周线中轨及以下</span>
          <small>
            三项同时满足 <ArrowUpRight size={14} /> 进入关注区间
          </small>
        </div>
        <details className="watch-method">
          <summary>计算口径与检查时间</summary>
          <p>
            北京时间 19:00 后检查当日收盘。BOLL(20,2)
            使用总体标准差，日线与周线分别计算；本周未结束时使用截至观察日的最后收盘。日线距下轨
            =（日收盘价 ÷ 日下轨 − 1）×100%，按日下轨价格衡量 ±2%，跌破超过 2%
            也不满足；周线按收盘价≤中轨判断，距离为（周收盘价 ÷ 周中轨 −
            1）×100%。
          </p>
          <p>
            TTM 为过去 365
            天已实施的税前现金分红，包含特别分红并按送转归一到当前股数；参考价只对应股息率条件。基本面与分红持续性在讨论中核查。
          </p>
        </details>
      </section>
      {error && !open && !active && (
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
          <div className="watch-summary" role="group" aria-label="观察名单筛选">
            {(
              [
                ["all", "全部公司", rows.length],
                ["attention", "关注区间", counts.attention],
                ["yield", "股息达标", counts.yield],
                ["bands", "日周条件达标", counts.bands],
                ["alerts", "未读变化", counts.alerts],
                ["incomplete", "需要补查", counts.incomplete],
              ] as const
            ).map(([key, label, count]) => (
              <button
                key={key}
                aria-pressed={filter === key}
                onClick={() => setFilter(key)}
              >
                <span>{label}</span>
                <strong>{count}</strong>
              </button>
            ))}
          </div>
          {rows.length > 0 && (
            <div className="watch-toolbar">
              <div className="watch-search">
                <Input
                  prefix={<Search size={16} />}
                  aria-label="搜索观察公司"
                  placeholder="搜索名称、代码或关注理由"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                />
              </div>
              <label className="watch-select">
                <SlidersHorizontal size={15} />
                <select
                  aria-label="持仓筛选"
                  value={ownership}
                  onChange={(e) => setOwnership(e.target.value)}
                >
                  <option value="all">全部持仓状态</option>
                  <option value="held">已持有</option>
                  <option value="unheld">未持有</option>
                </select>
              </label>
              <select
                aria-label="观察排序"
                value={sort}
                onChange={(e) => setSort(e.target.value as WatchSort)}
              >
                <option value="default">变化与关注优先</option>
                <option value="yield">股息率从高到低</option>
                <option value="daily">最接近日线下轨</option>
                <option value="weekly">最接近周线中轨</option>
                <option value="name">公司名称</option>
              </select>
              <div
                className="watch-view-toggle"
                role="group"
                aria-label="展示方式"
              >
                <button
                  aria-label="紧凑列表"
                  aria-pressed={view === "compact"}
                  onClick={() => setView("compact")}
                >
                  <List size={17} />
                </button>
                <button
                  aria-label="详细卡片"
                  aria-pressed={view === "cards"}
                  onClick={() => setView("cards")}
                >
                  <LayoutGrid size={17} />
                </button>
              </div>
            </div>
          )}
          {rows.length > 0 && (
            <div className="watch-result-meta">
              <span aria-live="polite">
                显示 {visible.length} / {rows.length} 家 · 金额单位：元 ·
                绿色数值表示满足对应条件
              </span>
              {(query || filter !== "all" || ownership !== "all") && (
                <button className="text-button" onClick={reset}>
                  清除筛选
                </button>
              )}
            </div>
          )}
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
          {visible.length > 0 && (
            <WatchList
              rows={visible}
              view={view}
              held={held}
              busy={busy}
              minYield={minYield}
              tolerance={tolerance}
              onDetails={(row) => setExpanded(row.entry.symbol)}
              onAsk={ask}
              onEdit={(row) => edit(row.entry)}
              onRetry={() => void run(refreshWatch)}
              onRead={(row) =>
                void run(async () => {
                  await readWatchAlerts(row.entry.symbol);
                  return getWatchBoard();
                })
              }
            />
          )}
          {rows.length > 0 && visible.length === 0 && (
            <section className="panel watch-no-results">
              <h3>当前条件下没有公司</h3>
              <p>调整搜索或筛选，可以继续查看已有名单。</p>
              <button className="secondary" onClick={reset}>
                清除筛选
              </button>
            </section>
          )}
          {view === "compact" && visible.length > 0 && (
            <p className="watch-meta">
              5%
              参考价仅用于股息条件比较。打开公司可查看多档参考价、日周轨道和分红来源。
            </p>
          )}
          <WatchWeekly
            board={board}
            onAsk={weeklyAsk}
            onDetails={(row) => setExpanded(row.entry.symbol)}
          />
        </>
      )}
      {active && !open && (
        <WatchDetails
          row={active}
          snapshot={snapshot}
          minYield={minYield}
          tolerance={tolerance}
          busy={busy}
          error={error}
          onClose={() => setExpanded(null)}
          onAsk={() => {
            setExpanded(null);
            ask(active);
          }}
          onEdit={() => {
            setExpanded(null);
            edit(active.entry);
          }}
          onRemove={() => {
            if (
              window.confirm(
                `移除 ${active.entry.name} 的观察资料与观察历史？持仓和交易记录保留。`,
              )
            )
              void run(async () => {
                await deleteWatch(active.entry.symbol);
                setExpanded(null);
                return getWatchBoard();
              });
          }}
        />
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
              className="watch-editor"
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
                  <Input
                    required
                    disabled={editing || busy}
                    value={form.symbol}
                    placeholder="600036.SS"
                    onChange={(e) => change("symbol", e.target.value)}
                  />
                </label>
                <label>
                  公司名称
                  <Input
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
                <Input.TextArea
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
                <Button disabled={busy} onClick={close}>
                  取消
                </Button>
                <Button
                  htmlType="submit"
                  type="primary"
                  disabled={busy || !form.symbol.trim() || !form.name.trim()}
                >
                  保存观察公司
                </Button>
              </div>
            </form>
          </Drawer>
        )}
      </>
    </div>
  );
}
