import { useEffect, useMemo, useState } from "react";
import { useRequestGuard } from "../../lib/useRequestGuard";
import { AlertTriangle, BookMarked } from "lucide-react";
import { getMemories, saveMemoryPreference } from "../../api";
import type { MemoryCandidate } from "../../types";
import { PageHeader } from "../../components/PageHeader";

export type MemoryPreferenceDraft = {
  preference: MemoryCandidate["preference"];
  note: string;
};

export function MemoryCenter({ flash }: { flash: (message: string) => void }) {
  const [items, setItems] = useState<MemoryCandidate[]>([]);
  const [drafts, setDrafts] = useState<Record<string, MemoryPreferenceDraft>>(
    {},
  );
  const [filter, setFilter] = useState<"all" | "pinned" | "hidden">("all");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [savingId, setSavingId] = useState<string | null>(null);
  const [error, setError] = useState("");
  const loadRequest = useRequestGuard();

  const load = async () => {
    const isCurrent = loadRequest.begin();
    setLoading(true);
    setError("");
    try {
      const memories = await getMemories();
      if (!isCurrent()) return;
      setItems(memories);
      setDrafts(
        Object.fromEntries(
          memories.map((item) => [
            item.id,
            { preference: item.preference, note: item.preferenceNote },
          ]),
        ),
      );
    } catch (nextError) {
      if (isCurrent()) setError(String(nextError));
    } finally {
      if (isCurrent()) setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const visibleItems = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return items
      .filter((item) => filter === "all" || item.preference === filter)
      .filter(
        (item) =>
          !normalized ||
          `${item.title} ${item.summary} ${item.status} ${item.preferenceNote}`
            .toLocaleLowerCase()
            .includes(normalized),
      )
      .sort(
        (left, right) =>
          Number(right.preference === "pinned") -
            Number(left.preference === "pinned") ||
          right.occurredAt.localeCompare(left.occurredAt),
      );
  }, [filter, items, query]);

  const updateDraft = (id: string, patch: Partial<MemoryPreferenceDraft>) => {
    setDrafts((current) => ({
      ...current,
      [id]: {
        ...(current[id] ?? { preference: "default", note: "" }),
        ...patch,
      },
    }));
  };

  const persist = async (item: MemoryCandidate, reset = false) => {
    const draft = reset
      ? { preference: "default" as const, note: "" }
      : (drafts[item.id] ?? {
          preference: item.preference,
          note: item.preferenceNote,
        });
    setSavingId(item.id);
    setError("");
    try {
      const updated = await saveMemoryPreference(item.id, draft);
      setItems((current) =>
        current.map((candidate) =>
          candidate.id === item.id ? updated : candidate,
        ),
      );
      setDrafts((current) => ({
        ...current,
        [item.id]: {
          preference: updated.preference,
          note: updated.preferenceNote,
        },
      }));
      flash(
        updated.preference === "pinned"
          ? "已标记为长期保留，相关检索会提高权重"
          : updated.preference === "hidden"
            ? "已永久屏蔽，不会进入后续 AI 检索"
            : "已恢复默认记忆策略",
      );
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSavingId(null);
    }
  };

  const pinnedCount = items.filter(
    (item) => item.preference === "pinned",
  ).length;
  const hiddenCount = items.filter(
    (item) => item.preference === "hidden",
  ).length;

  return (
    <div className="page narrow memory-page">
      <PageHeader title="长期记忆" description="管理 AI 可以参考的历史经验。" />

      <section className="memory-overview">
        <article>
          <span>记忆总数</span>
          <strong>{items.length}</strong>
        </article>
        <article>
          <span>长期保留</span>
          <strong>{pinnedCount}</strong>
          <small>相关时优先参考</small>
        </article>
        <article>
          <span>永久屏蔽</span>
          <strong>{hiddenCount}</strong>
          <small>不再提供给 AI</small>
        </article>
      </section>

      <section className="panel memory-manager">
        <div className="panel-title">
          <div>
            <h2>记忆目录</h2>
          </div>
        </div>
        <div className="memory-toolbar">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索资产、经验或状态"
          />
          <div>
            {(["all", "pinned", "hidden"] as const).map((value) => (
              <button
                key={value}
                className={filter === value ? "selected" : ""}
                onClick={() => setFilter(value)}
              >
                {value === "all"
                  ? "全部"
                  : value === "pinned"
                    ? "长期保留"
                    : "已屏蔽"}
              </button>
            ))}
          </div>
        </div>
        {error && (
          <div className="error-box" role="alert">
            <AlertTriangle size={17} />
            {error}
            <button
              className="secondary"
              disabled={loading}
              onClick={() => void load()}
            >
              重新加载
            </button>
          </div>
        )}
        {loading && (
          <div className="empty">正在从本地原始记录重建记忆目录…</div>
        )}
        {!loading && !error && visibleItems.length === 0 && (
          <div className="empty">暂无记忆，完成决策或分析后自动生成。</div>
        )}
        <div className="memory-manager-list">
          {visibleItems.map((item) => {
            const draft = drafts[item.id] ?? {
              preference: item.preference,
              note: item.preferenceNote,
            };
            const changed =
              draft.preference !== item.preference ||
              draft.note.trim() !== item.preferenceNote;
            return (
              <article
                key={item.id}
                className={`${item.preference} ${item.contradiction ? "contradiction" : ""}`}
              >
                <div className="managed-memory-head">
                  <div>
                    <span>{item.kind === "decision" ? "决策" : "AI 分析"}</span>
                    <strong>{item.title}</strong>
                  </div>
                  <div>
                    {item.reviewed && <em>已复盘</em>}
                    {item.contradiction && <em className="counter">反证</em>}
                    {item.preference === "pinned" && <b>长期保留</b>}
                    {item.preference === "hidden" && (
                      <b className="hidden">已屏蔽</b>
                    )}
                  </div>
                </div>
                <p>{item.summary}</p>
                <small>
                  {new Date(item.occurredAt).toLocaleDateString("zh-CN")} ·{" "}
                  {item.status}
                </small>
                <div className="memory-preference-editor">
                  <label>
                    <span>长期策略</span>
                    <select
                      value={draft.preference}
                      onChange={(event) =>
                        updateDraft(item.id, {
                          preference: event.target
                            .value as MemoryCandidate["preference"],
                        })
                      }
                    >
                      <option value="default">默认参与相关检索</option>
                      <option value="pinned">长期保留并提高权重</option>
                      <option value="hidden">永久屏蔽</option>
                    </select>
                  </label>
                  <label>
                    <span>我的注释（不会改写原记录）</span>
                    <input
                      disabled={draft.preference === "default"}
                      maxLength={1000}
                      value={draft.note}
                      onChange={(event) =>
                        updateDraft(item.id, { note: event.target.value })
                      }
                      placeholder={
                        draft.preference === "default"
                          ? "选择长期保留或屏蔽后可填写"
                          : "例如：只适用于高波动主动仓位"
                      }
                    />
                  </label>
                  <div>
                    <button
                      className="text-button"
                      disabled={
                        savingId === item.id ||
                        (item.preference === "default" && !item.preferenceNote)
                      }
                      onClick={() => void persist(item, true)}
                    >
                      恢复默认
                    </button>
                    <button
                      className="secondary"
                      disabled={
                        savingId === item.id ||
                        !changed ||
                        draft.preference === "default"
                      }
                      onClick={() => void persist(item)}
                    >
                      {savingId === item.id ? "保存中…" : "保存策略"}
                    </button>
                  </div>
                </div>
              </article>
            );
          })}
        </div>
        <details className="inline-help">
          <summary>记忆如何使用</summary>
          <p>
            长期保留不是把内容升级为事实，也不会绕过问题相关性直接发送；历史 AI
            回答仍只是待验证线索。偏好和注释属于投资域数据，会进入端到端加密同步包。
          </p>
        </details>
      </section>
    </div>
  );
}
