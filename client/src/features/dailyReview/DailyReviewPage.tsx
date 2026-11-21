import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Bell,
  BookmarkPlus,
  Check,
  CheckCheck,
  Clock,
  ExternalLink,
  FileText,
  Globe,
  MessageSquare,
  PenLine,
  Play,
  RefreshCw,
  Settings2,
  Sparkles,
  TrendingUp,
} from "lucide-react";
import {
  getDailyReviewFeed,
  markDailyReviewRead,
  runDailyReviewNow,
  saveDailyReviewSettings,
} from "../../api/dailyReview";
import {
  getSystemReviews,
  saveInvestmentRule,
  saveSystemReview,
} from "../../api/reviews";
import { Money } from "../../components/AmountPrivacy";
import { Badge } from "../../components/Badge";
import { Callout } from "../../components/Callout";
import { Drawer } from "../../components/Drawer";
import { EmptyState } from "../../components/EmptyState";
import { LoadState } from "../../components/LoadState";
import { PageHeader } from "../../components/PageHeader";
import { localDateValue } from "../../lib/dates";
import { systemReviewText } from "../../lib/reviewText";
import { useRequestGuard } from "../../lib/useRequestGuard";
import type {
  DailyReviewFeedItem,
  DailyReviewFeedResponse,
  DailyReviewItemKind,
  DailyReviewSettings,
} from "../../types/ai";
import type {
  InvestmentRuleInput,
  Snapshot,
  SystemReviewRecord,
} from "../../types/user";
import "./dailyReview.css";

type StreamFilter =
  "all" | "unread" | "ai" | "portfolio" | "official" | "signals" | "manual";

type UnifiedEntry =
  | {
      type: "auto";
      id: string;
      day: string;
      createdAt: string;
      item: DailyReviewFeedItem;
    }
  | {
      type: "manual";
      id: string;
      day: string;
      createdAt: string;
      review: SystemReviewRecord;
    };

const filterTabs: { id: StreamFilter; label: string }[] = [
  { id: "all", label: "全部动态" },
  { id: "unread", label: "仅未读" },
  { id: "ai", label: "AI 复盘" },
  { id: "portfolio", label: "持仓变动" },
  { id: "official", label: "官方公告" },
  { id: "signals", label: "分红与买点" },
  { id: "manual", label: "我的随手记" },
];

const kindMeta: Record<DailyReviewItemKind, { author: string; badge: string }> =
  {
    portfolioSummary: { author: "持仓日结", badge: "账本事实" },
    holdingMove: { author: "持仓异动", badge: "账本事实" },
    officialAnnouncement: { author: "巨潮官方公告", badge: "法定披露" },
    dividendUpdate: { author: "分红动态", badge: "分红计划" },
    watchSignal: { author: "买点观察", badge: "规则信号" },
    aiInsight: { author: "AI 每日复盘", badge: "AI 解读" },
    externalClue: { author: "公开线索", badge: "联网检索" },
  };

function kindIcon(kind: DailyReviewItemKind) {
  switch (kind) {
    case "portfolioSummary":
    case "holdingMove":
      return <TrendingUp size={18} />;
    case "officialAnnouncement":
      return <FileText size={18} />;
    case "dividendUpdate":
    case "watchSignal":
      return <Bell size={18} />;
    case "externalClue":
      return <Globe size={18} />;
    case "aiInsight":
      return <Sparkles size={18} />;
  }
}

function formatClock(iso: string): string {
  const parsed = new Date(iso);
  if (Number.isNaN(parsed.getTime())) return iso;
  return parsed.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function matchesFilter(entry: UnifiedEntry, filter: StreamFilter): boolean {
  if (filter === "all") return true;
  if (filter === "manual") return entry.type === "manual";
  if (entry.type === "manual") return false;
  const item = entry.item;
  switch (filter) {
    case "unread":
      return !item.readAt;
    case "ai":
      return item.kind === "aiInsight" || item.kind === "externalClue";
    case "portfolio":
      return item.kind === "portfolioSummary" || item.kind === "holdingMove";
    case "official":
      return item.kind === "officialAnnouncement";
    case "signals":
      return item.kind === "dividendUpdate" || item.kind === "watchSignal";
  }
}

export function DailyReviewPage({
  snapshot,
  onAsk,
  onOpenSettings,
  flash,
}: {
  snapshot: Snapshot;
  onAsk: (question: string) => void;
  onOpenSettings: () => void;
  flash: (message: string, tone?: "success" | "danger") => void;
}) {
  const [feed, setFeed] = useState<DailyReviewFeedResponse | null>(null);
  const [manualReviews, setManualReviews] = useState<SystemReviewRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState("");
  const [running, setRunning] = useState(false);
  const [filter, setFilter] = useState<StreamFilter>("all");
  const [selectedDay, setSelectedDay] = useState("");
  const [selectedSymbol, setSelectedSymbol] = useState("");
  const [expandedIds, setExpandedIds] = useState<Record<string, boolean>>({});
  const [noteText, setNoteText] = useState("");
  const [postingNote, setPostingNote] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsDraft, setSettingsDraft] = useState<DailyReviewSettings>({
    enabled: true,
    scheduleTime: "07:30",
    scope: "holdingsAndWatch",
    webSearch: true,
  });
  const [savingSettings, setSavingSettings] = useState(false);
  const [ruleDraft, setRuleDraft] = useState<InvestmentRuleInput | null>(null);
  const [savingRule, setSavingRule] = useState(false);
  const requestGuard = useRequestGuard();

  const refresh = useCallback(async () => {
    const isCurrent = requestGuard.begin();
    setLoading(true);
    try {
      const [nextFeed, nextManual] = await Promise.all([
        getDailyReviewFeed({ limit: 80 }),
        getSystemReviews(),
      ]);
      if (!isCurrent()) return;
      setFeed(nextFeed);
      setSettingsDraft(nextFeed.settings);
      setManualReviews(nextManual);
      setLoaded(true);
      setError("");
    } catch (nextError) {
      if (isCurrent()) setError(String(nextError));
    } finally {
      if (isCurrent()) setLoading(false);
    }
  }, [requestGuard]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleRunNow = async () => {
    if (running) return;
    setRunning(true);
    setError("");
    try {
      const result = await runDailyReviewNow(selectedDay || undefined);
      setFeed(result.feed);
      setSettingsDraft(result.feed.settings);
      if (result.run.status === "degraded") {
        flash(
          `已完成 ${result.run.reviewDay} 复盘（含 ${result.run.itemCount} 条动态，AI 已降级）`,
        );
      } else {
        flash(
          `已完成 ${result.run.reviewDay} 每日复盘，生成 ${result.run.itemCount} 条信息流动态`,
        );
      }
    } catch (nextError) {
      const msg = String(nextError);
      setError(msg);
      flash(msg, "danger");
    } finally {
      setRunning(false);
    }
  };

  const handleMarkRead = async (id?: string, all = false) => {
    try {
      const result = await markDailyReviewRead(all ? { all: true } : { id });
      const now = new Date().toISOString();
      setFeed((current) =>
        current
          ? {
              ...current,
              unreadCount: result.unreadCount,
              items: current.items.map((item) =>
                all || item.id === id
                  ? { ...item, readAt: item.readAt ?? now }
                  : item,
              ),
            }
          : current,
      );
      if (all) flash("已将全部复盘动态标为已读");
    } catch (nextError) {
      flash(String(nextError), "danger");
    }
  };

  const handlePostNote = async () => {
    if (postingNote || !noteText.trim()) return;
    setPostingNote(true);
    try {
      const created = await saveSystemReview({ content: noteText });
      setManualReviews((prev) => [created, ...prev]);
      setNoteText("");
      flash("复盘随手记已发布到时间流");
    } catch (nextError) {
      flash(String(nextError), "danger");
    } finally {
      setPostingNote(false);
    }
  };

  const handleSaveSettings = async () => {
    if (savingSettings) return;
    setSavingSettings(true);
    try {
      const saved = await saveDailyReviewSettings(settingsDraft);
      setFeed((current) =>
        current ? { ...current, settings: saved } : current,
      );
      setSettingsDraft(saved);
      setSettingsOpen(false);
      flash("每日自动复盘设置已保存");
    } catch (nextError) {
      flash(String(nextError), "danger");
    } finally {
      setSavingSettings(false);
    }
  };

  const handleSaveRule = async () => {
    if (!ruleDraft || savingRule) return;
    if (
      !ruleDraft.statement.trim() ||
      !ruleDraft.trigger.trim() ||
      !ruleDraft.rationale.trim()
    )
      return;
    setSavingRule(true);
    try {
      await saveInvestmentRule({
        ...ruleDraft,
        statement: ruleDraft.statement.trim(),
        trigger: ruleDraft.trigger.trim(),
        rationale: ruleDraft.rationale.trim(),
      });
      setRuleDraft(null);
      flash("已沉淀为个人投资规则（可在参考信息 → 投资规则查看）");
    } catch (nextError) {
      flash(String(nextError), "danger");
    } finally {
      setSavingRule(false);
    }
  };

  const trackedSymbols = useMemo(() => {
    const map = new Map<string, string>();
    for (const holding of snapshot.holdings) {
      if (holding.symbol) map.set(holding.symbol, holding.name);
    }
    for (const item of feed?.items ?? []) {
      if (item.symbol && !map.has(item.symbol)) {
        map.set(item.symbol, item.assetName || item.symbol);
      }
    }
    return Array.from(map.entries()).map(([symbol, name]) => ({
      symbol,
      name,
    }));
  }, [snapshot.holdings, feed?.items]);

  const availableDays = useMemo(() => {
    const set = new Set<string>();
    for (const run of feed?.runs ?? []) set.add(run.reviewDay);
    for (const item of feed?.items ?? []) set.add(item.reviewDay);
    for (const review of manualReviews) {
      set.add(localDateValue(new Date(review.createdAt)));
    }
    return Array.from(set).sort((a, b) => b.localeCompare(a));
  }, [feed?.runs, feed?.items, manualReviews]);

  const unifiedEntries = useMemo<UnifiedEntry[]>(() => {
    const autoEntries: UnifiedEntry[] = (feed?.items ?? []).map((item) => ({
      type: "auto",
      id: item.id,
      day: item.reviewDay,
      createdAt: item.createdAt,
      item,
    }));
    const manualEntries: UnifiedEntry[] = manualReviews.map((review) => ({
      type: "manual",
      id: `manual:${review.id}`,
      day: localDateValue(new Date(review.createdAt)),
      createdAt: review.createdAt,
      review,
    }));
    const importanceOrder = { high: 0, medium: 1, normal: 2 } as const;
    return [...autoEntries, ...manualEntries]
      .filter((entry) => {
        if (!matchesFilter(entry, filter)) return false;
        if (selectedDay && entry.day !== selectedDay) return false;
        if (selectedSymbol) {
          if (entry.type === "manual") {
            return systemReviewText(entry.review).includes(selectedSymbol);
          }
          return entry.item.symbol === selectedSymbol;
        }
        return true;
      })
      .sort((a, b) => {
        if (a.day !== b.day) return b.day.localeCompare(a.day);
        if (a.type === "auto" && b.type === "auto") {
          const rankDiff =
            importanceOrder[a.item.importance] -
            importanceOrder[b.item.importance];
          if (rankDiff !== 0) return rankDiff;
        }
        return b.createdAt.localeCompare(a.createdAt);
      });
  }, [feed?.items, manualReviews, filter, selectedDay, selectedSymbol]);

  const latestRun = feed?.runs[0];

  if (!loaded && (loading || error)) {
    return (
      <div className="page narrow daily-review-page">
        <PageHeader title="每日复盘" />
        <LoadState
          loading={loading}
          error={error}
          onRetry={() => void refresh()}
        />
      </div>
    );
  }

  return (
    <div className="page narrow daily-review-page">
      <PageHeader title="每日复盘" />

      <section className="daily-review-hero" aria-label="每日复盘状态">
        <div className="daily-review-status-meta">
          <span
            className={`daily-review-status-pill ${feed?.settings.enabled ? "" : "paused"}`}
          >
            <Clock size={14} />
            {feed?.settings.enabled
              ? `每日 ${feed.settings.scheduleTime} 自动复盘前一日`
              : "自动复盘已暂停"}
          </span>
          <span>
            范围：
            {feed?.settings.scope === "holdingsOnly"
              ? "仅当前持仓"
              : "当前持仓 + 个人自选"}
          </span>
          {latestRun && (
            <span>
              最近复盘：{latestRun.reviewDay}（{latestRun.itemCount} 条动态
              {latestRun.announcementCount > 0
                ? ` · ${latestRun.announcementCount} 条官方公告`
                : ""}
              ）
            </span>
          )}
        </div>
        <div className="daily-review-actions">
          <button
            type="button"
            className="primary"
            disabled={running}
            onClick={() => void handleRunNow()}
          >
            {running ? (
              <RefreshCw size={15} className="spin" />
            ) : (
              <Play size={15} />
            )}
            {running ? "正在生成复盘…" : "立即复盘"}
          </button>
          {(feed?.unreadCount ?? 0) > 0 && (
            <button
              type="button"
              className="secondary"
              onClick={() => void handleMarkRead(undefined, true)}
            >
              <CheckCheck size={15} />
              全部已读 ({feed?.unreadCount})
            </button>
          )}
          <button
            type="button"
            className="secondary"
            onClick={() => setSettingsOpen(true)}
          >
            <Settings2 size={15} />
            复盘设置
          </button>
        </div>
      </section>

      {error && (
        <Callout
          tone="danger"
          action={<button onClick={() => void refresh()}>重试</button>}
        >
          {error}
        </Callout>
      )}

      {latestRun?.status === "degraded" && latestRun.degradedReason && (
        <Callout
          tone="warning"
          action={
            latestRun.aiStatus === "skippedNoModel" ? (
              <button
                type="button"
                className="text-button"
                onClick={onOpenSettings}
              >
                去设置模型
              </button>
            ) : undefined
          }
        >
          <strong>最近一次复盘处于降级模式：</strong>
          {latestRun.degradedReason}
        </Callout>
      )}

      <section
        className="daily-review-composer"
        aria-label="发布个人复盘随手记"
      >
        <textarea
          aria-label="写一条今日复盘想法"
          value={noteText}
          disabled={postingNote}
          onChange={(event) => setNoteText(event.target.value)}
          placeholder="像发微博一样记下今天的持仓思考、顾虑或复盘心得，随时可沉淀为投资规则…"
        />
        <div className="daily-review-composer-footer">
          <span className="daily-review-composer-hint">
            发布后作为个人复盘记录长期保存在同一时间流中
          </span>
          <button
            type="button"
            className="primary"
            disabled={postingNote || !noteText.trim()}
            onClick={() => void handlePostNote()}
          >
            <PenLine size={15} />
            {postingNote ? "发布中…" : "发布随手记"}
          </button>
        </div>
      </section>

      <section className="daily-review-filters" aria-label="信息流筛选">
        <div
          className="daily-review-filter-row"
          role="group"
          aria-label="动态类型筛选"
        >
          {filterTabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              className="feed-chip"
              aria-pressed={filter === tab.id}
              onClick={() => setFilter(tab.id)}
            >
              {tab.label}
              {tab.id === "unread" && (feed?.unreadCount ?? 0) > 0 && (
                <span className="feed-chip-count">{feed?.unreadCount}</span>
              )}
            </button>
          ))}
          {availableDays.length > 0 && (
            <label className="daily-review-day-select">
              <span>日期</span>
              <select
                aria-label="按复盘日期筛选"
                value={selectedDay}
                onChange={(event) => setSelectedDay(event.target.value)}
              >
                <option value="">全部日期</option>
                {availableDays.map((day) => (
                  <option key={day} value={day}>
                    {day}
                  </option>
                ))}
              </select>
            </label>
          )}
        </div>
        {trackedSymbols.length > 0 && (
          <div
            className="daily-review-filter-row"
            role="group"
            aria-label="按标的筛选"
          >
            <button
              type="button"
              className="feed-chip symbol-chip"
              aria-pressed={selectedSymbol === ""}
              onClick={() => setSelectedSymbol("")}
            >
              全部标的
            </button>
            {trackedSymbols.slice(0, 12).map((stock) => (
              <button
                key={stock.symbol}
                type="button"
                className="feed-chip symbol-chip"
                aria-pressed={selectedSymbol === stock.symbol}
                onClick={() =>
                  setSelectedSymbol((current) =>
                    current === stock.symbol ? "" : stock.symbol,
                  )
                }
              >
                {stock.name}
              </button>
            ))}
          </div>
        )}
      </section>

      <section className="daily-review-stream" aria-label="每日复盘信息流">
        {unifiedEntries.length === 0 ? (
          <EmptyState
            title={
              filter === "all" && !selectedDay && !selectedSymbol
                ? "还没有每日复盘动态"
                : "当前筛选条件下没有动态"
            }
            action={
              <button
                type="button"
                className="primary"
                disabled={running}
                onClick={() => void handleRunNow()}
              >
                <Play size={15} />
                立即生成今日复盘流
              </button>
            }
          >
            {filter === "all" && !selectedDay && !selectedSymbol
              ? "点击「立即复盘」马上梳理昨日持仓变动、巨潮官方公告与 AI 复盘要点，之后每日将按设定时间自动更新。"
              : "可切换筛选条件或点击「立即复盘」生成最新内容。"}
          </EmptyState>
        ) : (
          unifiedEntries.map((entry, index) => {
            const showDayHeader =
              index === 0 || unifiedEntries[index - 1].day !== entry.day;
            if (entry.type === "manual") {
              const text = systemReviewText(entry.review);
              const firstLine = text.split("\n")[0].slice(0, 80);
              return (
                <div key={entry.id}>
                  {showDayHeader && (
                    <div className="feed-day-divider">{entry.day}</div>
                  )}
                  <article className="feed-card">
                    <div className="feed-card-head">
                      <div className="feed-author">
                        <span className="feed-avatar kind-manualNote">
                          <PenLine size={17} />
                        </span>
                        <div className="feed-author-meta">
                          <div className="feed-author-line">
                            <span className="feed-author-name">
                              我的复盘随手记
                            </span>
                            <Badge>个人笔记</Badge>
                          </div>
                          <span className="feed-time">
                            {formatClock(entry.createdAt)}
                          </span>
                        </div>
                      </div>
                    </div>
                    <h3 className="feed-card-title">
                      {firstLine || "复盘记录"}
                    </h3>
                    <p className="feed-card-body">{text}</p>
                    <div className="feed-card-footer">
                      <div className="feed-tags">
                        <span className="feed-tag">#手工复盘</span>
                      </div>
                      <div className="feed-footer-actions">
                        <button
                          type="button"
                          className="feed-action-btn primary-link"
                          onClick={() =>
                            onAsk(
                              `请结合我这篇个人复盘笔记和当前持仓、投资规则给出客观反馈：\n\n${text}`,
                            )
                          }
                        >
                          <MessageSquare size={14} />问 AI 讨论
                        </button>
                        <button
                          type="button"
                          className="feed-action-btn"
                          onClick={() =>
                            setRuleDraft({
                              category: "复盘",
                              statement: "",
                              trigger: "下次遇到相似情形时",
                              rationale: text,
                              active: true,
                              sourceReviewId: entry.review.id,
                            })
                          }
                        >
                          <BookmarkPlus size={14} />
                          沉淀为规则
                        </button>
                      </div>
                    </div>
                  </article>
                </div>
              );
            }

            const item = entry.item;
            const meta = kindMeta[item.kind];
            const isUnread = !item.readAt;
            const expanded = Boolean(expandedIds[item.id]);
            const hasLongBody =
              item.body.trim().length > 0 &&
              item.body.trim() !== item.summary.trim();

            return (
              <div key={entry.id}>
                {showDayHeader && (
                  <div className="feed-day-divider">{entry.day} 复盘</div>
                )}
                <article className={`feed-card ${isUnread ? "unread" : ""}`}>
                  <div className="feed-card-head">
                    <div className="feed-author">
                      <span className={`feed-avatar kind-${item.kind}`}>
                        {kindIcon(item.kind)}
                      </span>
                      <div className="feed-author-meta">
                        <div className="feed-author-line">
                          <span className="feed-author-name">
                            {meta.author}
                          </span>
                          <Badge
                            tone={
                              item.importance === "high"
                                ? "warning"
                                : item.origin === "officialFiling"
                                  ? "info"
                                  : "neutral"
                            }
                          >
                            {meta.badge}
                          </Badge>
                          {item.importance === "high" && (
                            <Badge tone="warning">重要</Badge>
                          )}
                          {isUnread && (
                            <span
                              className="feed-unread-dot"
                              aria-label="未读动态"
                            />
                          )}
                        </div>
                        <span className="feed-time">
                          复盘日 {item.reviewDay} ·{" "}
                          {formatClock(item.createdAt)}
                        </span>
                      </div>
                    </div>
                    <div className="feed-head-badges">
                      {item.assetName && (
                        <Badge>
                          {item.assetName}
                          {item.symbol ? ` (${item.symbol})` : ""}
                        </Badge>
                      )}
                    </div>
                  </div>

                  <h3 className="feed-card-title">{item.title}</h3>
                  <p className="feed-card-summary">{item.summary}</p>

                  {item.metrics.length > 0 && (
                    <div className="feed-metrics">
                      {item.metrics.map((metric, idx) => (
                        <div
                          key={`${metric.label}-${idx}`}
                          className={`feed-metric tone-${metric.tone}`}
                        >
                          <span className="feed-metric-label">
                            {metric.label}
                          </span>
                          {metric.sensitive ? (
                            <Money className="feed-metric-value">
                              {metric.value}
                            </Money>
                          ) : (
                            <span className="feed-metric-value">
                              {metric.value}
                            </span>
                          )}
                        </div>
                      ))}
                    </div>
                  )}

                  {hasLongBody && expanded && (
                    <p className="feed-card-body">{item.body}</p>
                  )}

                  {item.sources.length > 0 && (
                    <div className="feed-sources">
                      {item.sources.map((src, idx) =>
                        src.url ? (
                          <a
                            key={`${src.url}-${idx}`}
                            className="feed-source-link"
                            href={src.url}
                            target="_blank"
                            rel="noopener noreferrer"
                          >
                            <ExternalLink size={13} />
                            <span>
                              {src.publisher ? `[${src.publisher}] ` : ""}
                              {src.label}
                              {src.publishedOn ? ` (${src.publishedOn})` : ""}
                            </span>
                          </a>
                        ) : (
                          <span
                            key={`${src.label}-${idx}`}
                            className="feed-time"
                          >
                            {src.publisher ? `[${src.publisher}] ` : ""}
                            {src.label}
                          </span>
                        ),
                      )}
                    </div>
                  )}

                  <div className="feed-card-footer">
                    <div className="feed-tags">
                      {item.tags.map((tag) => (
                        <span key={tag} className="feed-tag">
                          #{tag}
                        </span>
                      ))}
                    </div>
                    <div className="feed-footer-actions">
                      {hasLongBody && (
                        <button
                          type="button"
                          className="feed-action-btn"
                          onClick={() =>
                            setExpandedIds((prev) => ({
                              ...prev,
                              [item.id]: !prev[item.id],
                            }))
                          }
                        >
                          {expanded ? "收起详情" : "展开全文"}
                        </button>
                      )}
                      {isUnread && (
                        <button
                          type="button"
                          className="feed-action-btn"
                          onClick={() => void handleMarkRead(item.id, false)}
                        >
                          <Check size={14} />
                          已读
                        </button>
                      )}
                      <button
                        type="button"
                        className="feed-action-btn primary-link"
                        onClick={() =>
                          onAsk(
                            item.askPrompt ||
                              `请结合我的持仓与投资规则深入分析「${item.title}」：${item.summary}`,
                          )
                        }
                      >
                        <MessageSquare size={14} />问 AI 深聊
                      </button>
                      <button
                        type="button"
                        className="feed-action-btn"
                        onClick={() =>
                          setRuleDraft({
                            category: "复盘",
                            statement: "",
                            trigger: item.assetName
                              ? `跟踪 ${item.assetName} 类似情形时`
                              : "下次复盘类似情形时",
                            rationale: `${item.title}：${item.summary}`,
                            active: true,
                          })
                        }
                      >
                        <BookmarkPlus size={14} />
                        沉淀为规则
                      </button>
                    </div>
                  </div>
                </article>
              </div>
            );
          })
        )}
      </section>

      {settingsOpen && (
        <Drawer
          title="每日自动复盘设置"
          busy={savingSettings}
          onClose={() => setSettingsOpen(false)}
        >
          <div className="daily-review-settings-form">
            <label className="daily-review-toggle">
              <div>
                <span>开启每日自动复盘</span>
                <small>
                  每日到达设定时间后自动拉取前一日持仓变动与巨潮官方公告，并调用
                  AI 生成复盘动态
                </small>
              </div>
              <input
                type="checkbox"
                checked={settingsDraft.enabled}
                onChange={(event) =>
                  setSettingsDraft((prev) => ({
                    ...prev,
                    enabled: event.target.checked,
                  }))
                }
              />
            </label>

            <label className="daily-review-setting-row">
              <span>
                每日自动触发时间（账号时区 {feed?.timezone || "Asia/Shanghai"}）
              </span>
              <input
                type="time"
                value={settingsDraft.scheduleTime}
                onChange={(event) =>
                  setSettingsDraft((prev) => ({
                    ...prev,
                    scheduleTime: event.target.value,
                  }))
                }
              />
            </label>

            <label className="daily-review-setting-row">
              <span>复盘跟踪范围</span>
              <select
                value={settingsDraft.scope}
                onChange={(event) =>
                  setSettingsDraft((prev) => ({
                    ...prev,
                    scope: event.target.value as DailyReviewSettings["scope"],
                  }))
                }
              >
                <option value="holdingsAndWatch">当前持仓 + 个人自选</option>
                <option value="holdingsOnly">仅当前持仓</option>
              </select>
            </label>

            <label className="daily-review-toggle">
              <div>
                <span>允许公开联网检索</span>
                <small>
                  仅发送公开股票代码与公司名称检索近期行业/公司公开线索，不发送个人金额与账本
                </small>
              </div>
              <input
                type="checkbox"
                checked={settingsDraft.webSearch}
                onChange={(event) =>
                  setSettingsDraft((prev) => ({
                    ...prev,
                    webSearch: event.target.checked,
                  }))
                }
              />
            </label>

            <div className="form-actions">
              <button
                type="button"
                className="primary"
                disabled={savingSettings}
                onClick={() => void handleSaveSettings()}
              >
                {savingSettings ? "保存中…" : "保存设置"}
              </button>
            </div>

            {(feed?.runs.length ?? 0) > 0 && (
              <div>
                <strong>近期自动/手动运行记录</strong>
                <div className="daily-review-run-history">
                  {feed?.runs.slice(0, 8).map((run) => (
                    <div key={run.id} className="daily-review-run-item">
                      <span>
                        {run.reviewDay} ·{" "}
                        {run.trigger === "scheduled" ? "定时" : "手动"} ·{" "}
                        {run.itemCount} 条
                      </span>
                      <Badge
                        tone={
                          run.status === "completed"
                            ? "success"
                            : run.status === "degraded"
                              ? "warning"
                              : "neutral"
                        }
                      >
                        {run.status === "completed"
                          ? "完整完成"
                          : run.status === "degraded"
                            ? "已降级"
                            : "失败"}
                      </Badge>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        </Drawer>
      )}

      {ruleDraft && (
        <Drawer
          title="从复盘动态沉淀投资规则"
          busy={savingRule}
          onClose={() => setRuleDraft(null)}
        >
          <fieldset
            className="rule-editor holding-fields"
            disabled={savingRule}
          >
            <label>
              <span>类别</span>
              <select
                value={ruleDraft.category}
                onChange={(e) =>
                  setRuleDraft({
                    ...ruleDraft,
                    category: e.target.value as InvestmentRuleInput["category"],
                  })
                }
              >
                {["资产配置", "风险", "研究", "仓位", "行为", "复盘"].map(
                  (value) => (
                    <option key={value}>{value}</option>
                  ),
                )}
              </select>
            </label>
            <label>
              <span>规则内容</span>
              <textarea
                value={ruleDraft.statement}
                onChange={(e) =>
                  setRuleDraft({ ...ruleDraft, statement: e.target.value })
                }
                placeholder="例如：出现重大分红方案变化时，先核对自由现金流覆盖率再决定加减仓。"
              />
            </label>
            <label>
              <span>触发条件</span>
              <textarea
                value={ruleDraft.trigger}
                onChange={(e) =>
                  setRuleDraft({ ...ruleDraft, trigger: e.target.value })
                }
              />
            </label>
            <label>
              <span>依据</span>
              <textarea
                value={ruleDraft.rationale}
                onChange={(e) =>
                  setRuleDraft({ ...ruleDraft, rationale: e.target.value })
                }
              />
            </label>
            <div className="rule-editor-actions">
              <button
                type="button"
                className="primary"
                disabled={
                  savingRule ||
                  !ruleDraft.statement.trim() ||
                  !ruleDraft.trigger.trim() ||
                  !ruleDraft.rationale.trim()
                }
                onClick={() => void handleSaveRule()}
              >
                {savingRule ? "保存中…" : "建立规则"}
              </button>
            </div>
          </fieldset>
        </Drawer>
      )}
    </div>
  );
}
