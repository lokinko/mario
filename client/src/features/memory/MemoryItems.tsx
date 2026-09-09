import type { MemoryCandidate } from "../../types";

export function MemoryItems({
  items,
  compact = false,
  excludedIds = [],
  onToggle,
}: {
  items: MemoryCandidate[];
  compact?: boolean;
  excludedIds?: string[];
  onToggle?: (id: string) => void;
}) {
  return (
    <div className={`memory-candidate-list ${compact ? "compact" : ""}`}>
      {items.map((item) => {
        const selected = onToggle
          ? !excludedIds.includes(item.id)
          : item.selected;
        return (
          <article
            key={`${item.kind}-${item.id}`}
            className={`${item.contradiction ? "contradiction" : ""} ${selected ? "" : "excluded"}`}
          >
            <div className="memory-head">
              <div>
                <span>{item.kind === "decision" ? "决策" : "AI 分析"}</span>
                <strong>{item.title}</strong>
              </div>
              <div>
                {item.reviewed && <em>已复盘</em>}
                {item.contradiction && <em className="counter">反证</em>}
                {item.preference === "pinned" && <em>长期保留</em>}
                {item.retrieval && <b>{item.retrieval.score.toFixed(1)} 分</b>}
                {onToggle && (
                  <button
                    className={
                      selected ? "memory-toggle selected" : "memory-toggle"
                    }
                    onClick={() => onToggle(item.id)}
                  >
                    {selected ? "本次发送" : "留在本机"}
                  </button>
                )}
              </div>
            </div>
            <p>
              {item.summary}
              {item.preferenceNote ? ` · 我的注释：${item.preferenceNote}` : ""}
            </p>
            <div className="memory-reasons">
              {item.retrieval?.reasons.map((reason) => (
                <span key={reason}>{reason}</span>
              ))}
            </div>
            <footer>
              <small>
                {new Date(item.occurredAt).toLocaleDateString("zh-CN")} ·{" "}
                {item.status}
              </small>
              {selected && item.retrieval ? (
                <small>{item.retrieval.passes.join(" + ") || "候选初筛"}</small>
              ) : (
                <small className="local-memory">不会进入模型</small>
              )}
            </footer>
            {!compact && (
              <details>
                <summary>查看冻结的结构化内容</summary>
                <pre>{JSON.stringify(item.content, null, 2)}</pre>
              </details>
            )}
          </article>
        );
      })}
    </div>
  );
}
