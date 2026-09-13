import { useEffect, useRef, useState } from "react";
import { getSnapshot, updateHoldingAmount } from "../../api";
import type { Holding, Snapshot } from "../../types";

export function AssetAmountRow({
  holding,
  revision,
  onUpdate,
  onEdit,
}: {
  holding: Holding;
  revision?: string;
  onUpdate: (snapshot: Snapshot) => void;
  onEdit: () => void;
}) {
  const [draft, setDraft] = useState(String(holding.marketValue));
  const [status, setStatus] = useState("");
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const dirty = useRef(false);
  const pending = useRef(false);
  const version = useRef(revision);
  const attempt = useRef<{ value: number; id: string } | undefined>(undefined);
  useEffect(() => {
    // A remote refresh must never silently authorize a draft based on old data.
    if (!dirty.current && !pending.current) {
      version.current = revision;
      setDraft(String(holding.marketValue));
    }
  }, [holding.marketValue, revision]);
  const save = async () => {
    if (pending.current || !dirty.current) return;
    const value = Number(draft);
    if (!draft.trim() || !Number.isFinite(value) || value < 0 || value > 1e15) {
      setStatus("请输入有效金额；清零请填 0");
      setFailed(true);
      return;
    }
    pending.current = true;
    setBusy(true);
    setFailed(false);
    setStatus("保存中…");
    try {
      if (!version.current) throw new Error("请先读取最新资产，再保存金额");
      if (!attempt.current || attempt.current.value !== value)
        attempt.current = { value, id: crypto.randomUUID() };
      const result = await updateHoldingAmount(
        holding.id,
        value,
        version.current,
        attempt.current.id,
      );
      version.current = result.holdingRevisions?.[holding.id];
      dirty.current = false;
      attempt.current = undefined;
      onUpdate(result);
      setStatus("已保存");
    } catch (e) {
      setStatus(String(e));
      setFailed(true);
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };
  const refresh = async () => {
    try {
      const result = await getSnapshot();
      onUpdate(result);
      version.current = result.holdingRevisions?.[holding.id];
      const current = result.holdings.find((h) => h.id === holding.id);
      setStatus(
        current
          ? `最新金额 ${current.marketValue} ${current.currency}；核对后点重试保存你的输入`
          : "该资产已移除，无法保存",
      );
      attempt.current = undefined;
    } catch (e) {
      setStatus(String(e));
    }
  };
  return (
    <div className="asset-amount-row">
      <button
        className="text-button asset-name"
        onClick={onEdit}
        aria-label={`编辑 ${holding.name}`}
      >
        <strong>{holding.name}</strong>
        <small>确认于 {holding.valuationDate || "日期待补充"}</small>
      </button>
      <div className="asset-amount-editor">
        <label>
          <span className="sr-only">{holding.name}金额</span>
          <small>{holding.currency}</small>
          <input
            type="text"
            inputMode="decimal"
            aria-label={`${holding.name}金额`}
            title="回车或离开输入框保存"
            value={draft}
            readOnly={busy}
            onChange={(e) => {
              setDraft(e.target.value);
              dirty.current = true;
              setFailed(false);
              setStatus("待保存");
            }}
            onBlur={() => void save()}
            onKeyDown={(e) => {
              if (e.key === "Tab") {
                const fields = Array.from(
                  document.querySelectorAll<HTMLInputElement>(
                    ".asset-amount-editor input",
                  ),
                );
                const next =
                  fields[
                    fields.indexOf(e.currentTarget) + (e.shiftKey ? -1 : 1)
                  ];
                if (next) {
                  e.preventDefault();
                  next.focus();
                }
              }
              if (e.key === "Enter") {
                e.preventDefault();
                void save();
              }
            }}
          />
        </label>
        <small role={failed ? "alert" : "status"}>{status}</small>
        {failed && (
          <span className="asset-save-actions">
            <button className="text-button" onClick={() => void save()}>
              重试
            </button>
            <button className="text-button" onClick={() => void refresh()}>
              核对最新金额
            </button>
          </span>
        )}
      </div>
    </div>
  );
}
