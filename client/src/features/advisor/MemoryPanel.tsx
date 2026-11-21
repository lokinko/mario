import { useCallback, useEffect, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { Badge } from "../../components/Badge";
import { Callout } from "../../components/Callout";
import { DataStamp } from "../../components/DataStamp";
import { deleteMemory, getMemory, setMemoryStatus } from "../../api/memory";
import type { MemoryKind, MemoryNote, MemoryOverview } from "../../types/ai";

const kinds: [MemoryKind, string][] = [
  ["preference", "偏好"],
  ["viewpoint", "观点"],
  ["context", "背景"],
];

function Note({
  note,
  busy,
  onToggle,
  onDelete,
  onOpenAnalysis,
}: {
  note: MemoryNote;
  busy: boolean;
  onToggle: () => void;
  onDelete: () => void;
  onOpenAnalysis: (id: string) => void;
}) {
  return (
    <li className={note.active ? "" : "muted"}>
      <p>
        {note.statement}{" "}
        <Badge tone={note.origin === "userStated" ? "info" : "warning"}>
          {note.origin === "userStated" ? "你的原话" : "模型归纳"}
        </Badge>
        {!note.active && <Badge>已归档</Badge>}
      </p>
      <p className="muted">理由：{note.rationale}</p>
      {note.quote && <blockquote>{note.quote}</blockquote>}
      <p className="muted">
        来源：
        {note.sourceAnalysisIds.map((id, index) => (
          <button
            key={id}
            type="button"
            className="text-button"
            onClick={() => onOpenAnalysis(id)}
          >
            对话 {index + 1}
          </button>
        ))}
        · <DataStamp date={new Date(note.updatedAt).toLocaleString("zh-CN")} />
      </p>
      <div className="form-actions">
        <button
          type="button"
          className="text-button"
          disabled={busy}
          onClick={onToggle}
        >
          {note.active ? "归档" : "恢复"}
        </button>
        <button
          type="button"
          className="text-button danger-text"
          disabled={busy}
          onClick={onDelete}
        >
          删除
        </button>
      </div>
    </li>
  );
}

/** Lets the user inspect, archive and delete what the model remembers. */
export function MemoryPanel({
  dreaming,
  dreamError,
  onDream,
  onOpenAnalysis,
}: {
  dreaming: boolean;
  dreamError: string;
  onDream: () => void;
  onOpenAnalysis: (id: string) => void;
}) {
  const [overview, setOverview] = useState<MemoryOverview>();
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const load = useCallback(() => {
    setError("");
    return getMemory()
      .then(setOverview)
      .catch((nextError) => setError(String(nextError)));
  }, []);
  useEffect(() => {
    if (!dreaming) void load();
  }, [dreaming, load]);

  const change = async (id: string, action: () => Promise<unknown>) => {
    setBusy(id);
    setError("");
    try {
      await action();
      await load();
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusy("");
    }
  };

  const last = overview?.dream.lastResult;
  return (
    <div className="memory-panel">
      {!overview && !error && <p role="status">正在读取长期记忆…</p>}
      {overview && !overview.enabled && <p>问答预设中已关闭长期记忆。</p>}
      {overview && (
        <div className="form-actions">
          <span className="muted">
            {overview.pendingAnalyses
              ? `${overview.pendingAnalyses} 次对话尚未整理`
              : "所有对话均已整理"}
          </span>
          <button
            type="button"
            className="secondary"
            disabled={dreaming || !overview.pendingAnalyses}
            onClick={onDream}
          >
            {dreaming && <LoaderCircle size={16} className="spin" />}
            {dreaming ? "整理中…" : "立即整理"}
          </button>
        </div>
      )}
      {dreamError && <Callout tone="danger">{dreamError}</Callout>}
      {last && overview?.dream.lastRunAt && (
        <p className="muted">
          上次整理{" "}
          <DataStamp
            date={new Date(overview.dream.lastRunAt).toLocaleString("zh-CN")}
          />
          ：读取 {last.analyses} 次对话，新增 {last.added}、更新 {last.updated}
          、归档 {last.archived}
          {last.rejected.length > 0 && `，拒绝 ${last.rejected.length} 条`}
        </p>
      )}
      {!!last?.rejected.length && (
        <details>
          <summary>被拒绝的整理建议</summary>
          <ul>
            {last.rejected.map((reason, index) => (
              <li key={index}>{reason}</li>
            ))}
          </ul>
        </details>
      )}
      {overview?.items.length === 0 && <p className="empty">暂无长期记忆</p>}
      {kinds.map(([kind, label]) => {
        const items =
          overview?.items.filter((item) => item.kind === kind) ?? [];
        return items.length ? (
          <section key={kind} aria-label={label}>
            <h3>{label}</h3>
            <ul>
              {items.map((note) => (
                <Note
                  key={note.id}
                  note={note}
                  busy={Boolean(busy) || dreaming}
                  onOpenAnalysis={onOpenAnalysis}
                  onToggle={() =>
                    void change(note.id, () =>
                      setMemoryStatus(note.id, !note.active),
                    )
                  }
                  onDelete={() => {
                    if (window.confirm("删除后无法恢复，确定删除这条记忆？"))
                      void change(note.id, () => deleteMemory(note.id));
                  }}
                />
              ))}
            </ul>
          </section>
        ) : null;
      })}
      {error && <Callout tone="danger">{error}</Callout>}
    </div>
  );
}
