import { useEffect, useState } from "react";
import { Button, Input, Tabs } from "antd";
import {
  Plus,
  RefreshCw,
  Search,
  List,
  LayoutGrid,
  SlidersHorizontal,
} from "lucide-react";
import {
  getWatchBoard,
  getWatchCatalog,
  refreshWatchCatalog,
  type CatalogBoard,
  saveWatch,
  saveWatchGrid,
  setWatchPinned,
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

import { DividendCalendar } from "./DividendCalendar";
import { WatchCatalogEditor } from "./WatchCatalogEditor";
import { WatchList } from "./WatchList";
import { WatchCatalogDetails } from "./WatchCatalogDetails";
import { WatchDetails } from "./WatchDetails";
import { WatchWeekly } from "./WatchWeekly";
import {
  canonical,
  displayWatch,
  type WatchDisplayRow,
  statuses,
  selectRows,
  checks,
  sortOptions,
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
  const [catalog, setCatalog] = useState<CatalogBoard | null>(null);
  const [calendarOpen, setCalendarOpen] = useState(false);
  const [catalogEditing, setCatalogEditing] = useState(false);
  const [catalogError, setCatalogError] = useState("");
  const [group, setGroup] = useState("favorites");
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
  const catalogGuard = useRequestGuard();
  const loadCatalog = async () => {
    const current = catalogGuard.begin();
    try {
      const data = await getWatchCatalog();
      if (current()) {
        setCatalog(data);
        setCatalogError("");
      }
    } catch (e) {
      if (current()) setCatalogError(String(e));
    }
  };
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
    void loadCatalog();
  }, []);
  useEffect(() => {
    if (group === "favorites" || busy) return;
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void loadCatalog();
    }, 30000);
    return () => window.clearInterval(timer);
  }, [group, busy]);
  const run = async (action: () => Promise<WatchBoard | void>) => {
    if (busy) return false;
    const current = guard.begin();
    setBusy(true);
    setError("");
    try {
      const next = await action();
      if (!current()) return false;
      if (next) setBoard(next);
      return true;
    } catch (e) {
      if (current()) setError(String(e));
      return false;
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
  const edit = (entry?: WatchInput) => {
    setForm(
      entry
        ? { symbol: entry.symbol, name: entry.name, thesis: entry.thesis }
        : empty,
    );
    setEditing(Boolean(entry));
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
  const own = group === "favorites";
  const favorites = new Set(rows.map((row) => row.entry.symbol));
  const canAdd = rows.length < 50;
  const currentRows: WatchDisplayRow[] = own
    ? rows.map(displayWatch)
    : (catalog?.rows.filter((row) => row.industryId === group) ?? []);
  const publicActive = !own
    ? catalog?.rows.find((row) => row.company.symbol === expanded)
    : undefined;
  const refresh = () =>
    own
      ? run(refreshWatch)
      : run(async () => {
          const current = catalogGuard.begin();
          const next = await refreshWatchCatalog(group);
          if (current()) {
            setCatalog(next);
            setCatalogError("");
          }
        });
  const add = (row: WatchDisplayRow) => {
    if (!board || favorites.has(row.company.symbol) || !canAdd || busy) return;
    void run(async () => {
      await saveWatch(
        {
          symbol: row.company.symbol,
          name: row.company.name,
          thesis: "",
        },
        true,
      );
      flash(
        `已将${row.company.name}加入自选，收盘后自动检查，也可在自选中立即检查。`,
      );
      return getWatchBoard();
    });
  };
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
    yield: currentRows.filter((r) => checks(r, minYield)[0] === true).length,
    daily: currentRows.filter((r) => checks(r, minYield)[1] === true).length,
    weekly: currentRows.filter((r) => checks(r, minYield)[2] === true).length,
  };
  const visible = selectRows(
    currentRows,
    query,
    filter,
    sort,
    held,
    ownership,
    minYield,
  );
  const active = own
    ? rows.find((r) => r.entry.symbol === expanded)
    : undefined;
  const reset = () => {
    setQuery("");
    setFilter("all");
    setOwnership("all");
  };
  const changeGroup = (next: string) => {
    setGroup(next);
    setExpanded(null);
    setError("");
    reset();
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
        action={
          <>
            <Button
              disabled={busy || open || !!expanded || catalogEditing}
              onClick={() => setCalendarOpen(true)}
            >
              分红日历
            </Button>
            {catalog?.canEdit && (
              <Button
                disabled={busy || open || !!expanded}
                onClick={() => setCatalogEditing(true)}
              >
                维护公共公司
              </Button>
            )}
            <Button
              disabled={busy || !currentRows.length}
              onClick={() => void refresh()}
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
      {calendarOpen && (
        <DividendCalendar onClose={() => setCalendarOpen(false)} />
      )}
      {catalogEditing && catalog && (
        <WatchCatalogEditor
          board={catalog}
          initialIndustry={
            group === "favorites" ? catalog.catalog.industries[0].id : group
          }
          onClose={() => {
            setCatalogEditing(false);
            onSaved();
          }}
          onEdited={onEdited}
          onSaved={(data) => {
            catalogGuard.begin();
            setCatalog(data);
            setCatalogEditing(false);
            onSaved();
            flash("公共公司目录已更新，所有账号可见");
          }}
        />
      )}
      <Tabs
        className="watch-industry-tabs"
        aria-label="公司分组"
        activeKey={group}
        onChange={changeGroup}
        items={[
          { key: "favorites", label: `自选 ${rows.length}` },
          ...(catalog?.catalog.industries.map((industry) => ({
            key: industry.id,
            label: `${industry.name} ${industry.companies.length}`,
          })) ?? []),
        ]}
      />
      <section className="panel watch-intro">
        <div className="watch-rule-strip">
          <span className="yield">① TTM 税前股息 ≥{minYield}%</span>
          <span className="daily">② 日线下轨 ±{tolerance}%</span>
          <span className="weekly">③ 周线中轨及以下</span>
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
      {error && !open && !active && !publicActive && (
        <Callout tone="danger">
          <span>{error}</span>
          <button disabled={busy} onClick={() => void load()}>
            重新读取
          </button>
        </Callout>
      )}
      {catalogError && !open && !publicActive && (
        <Callout tone="danger">
          <span>行业目录读取失败：{catalogError}</span>
          <button disabled={busy} onClick={() => void loadCatalog()}>
            重新读取行业目录
          </button>
        </Callout>
      )}
      {!catalog && !catalogError && <p role="status">正在读取行业目录…</p>}
      {!board && !error && <p role="status">正在读取观察名单…</p>}
      {board && (
        <>
          <div className="watch-summary" role="group" aria-label="观察名单筛选">
            {(
              [
                ["all", "全部公司", currentRows.length],
                ["yield", "股息达标", counts.yield],
                ["daily", "日线达标", counts.daily],
                ["weekly", "周线达标", counts.weekly],
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
          {currentRows.length > 0 && (
            <div className="watch-toolbar">
              <div className="watch-search">
                <Input
                  prefix={<Search size={16} />}
                  aria-label="搜索观察公司"
                  placeholder={
                    own ? "搜索名称、代码或关注理由" : "搜索名称或代码"
                  }
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
              <label className="watch-sort">
                排序
                <select
                  aria-label="观察排序"
                  value={sort}
                  onChange={(e) => setSort(e.target.value as WatchSort)}
                >
                  {Object.entries(sortOptions).map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
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
          {currentRows.length > 0 && (
            <div className="watch-result-meta">
              <span className="sr-only" aria-live="polite">
                显示 {visible.length} / {currentRows.length} 家
              </span>
              {(query || filter !== "all" || ownership !== "all") && (
                <button className="text-button" onClick={reset}>
                  清除筛选
                </button>
              )}
            </div>
          )}
          {own && !rows.length && (
            <section className="panel">
              <h3>从一家公司开始</h3>
              <p>
                加入你愿意长期研究的公司；已有持仓可以直接选择，未持有的公司也能观察。
              </p>
              <button className="secondary" onClick={() => edit()}>
                加入第一家公司
              </button>
              {catalog && (
                <button
                  className="text-button"
                  onClick={() => changeGroup(catalog.catalog.industries[0].id)}
                >
                  按行业浏览公司
                </button>
              )}
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
              sort={sort}
              onSort={setSort}
              onPin={(row) =>
                void run(async () => {
                  const entry = await setWatchPinned(
                    row.entry.symbol,
                    !row.entry.pinned,
                  );
                  return {
                    ...board,
                    rows: board.rows.map((item) =>
                      item.entry.symbol === entry.symbol
                        ? { ...item, entry }
                        : item,
                    ),
                  };
                })
              }
              favorites={favorites}
              canAdd={canAdd}
              onAdd={add}
              onDetails={(row) => setExpanded(row.company.symbol)}
              onAsk={ask}
              onEdit={(row) => edit(row.entry)}
              onRetry={() => void refresh()}
              onRead={(row) =>
                void run(async () => {
                  await readWatchAlerts(row.entry.symbol);
                  return getWatchBoard();
                })
              }
            />
          )}
          {currentRows.length > 0 && visible.length === 0 && (
            <section className="panel watch-no-results">
              <h3>当前条件下没有公司</h3>
              <p>调整搜索或筛选，可以继续查看已有名单。</p>
              <button className="secondary" onClick={reset}>
                清除筛选
              </button>
            </section>
          )}
          {!own && !canAdd && (
            <p>自选已达 50 家，请先在自选中移除公司后再添加。</p>
          )}
          {own && (
            <WatchWeekly
              board={board}
              onAsk={weeklyAsk}
              onDetails={(row) => setExpanded(row.entry.symbol)}
            />
          )}
        </>
      )}
      {publicActive && catalog && !open && (
        <WatchCatalogDetails
          row={publicActive}
          industry={
            catalog.catalog.industries.find((industry) => industry.id === group)
              ?.name ?? ""
          }
          verifiedOn={catalog.catalog.verifiedOn}
          minYield={minYield}
          tolerance={tolerance}
          busy={busy}
          error={error || catalogError}
          joined={favorites.has(publicActive.company.symbol)}
          canAdd={canAdd}
          onClose={() => setExpanded(null)}
          onAdd={() => add(publicActive)}
          onRetry={() => void refresh()}
        />
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
          onGridEdited={onEdited}
          onGridClosed={onSaved}
          onGridSave={(plan) =>
            run(() => saveWatchGrid(active.entry.symbol, plan))
          }
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
