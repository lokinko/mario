import { useState } from "react";
import { Button, Input, Popover } from "antd";
import {
  getWatchBoard,
  saveWatchRules,
  type BandRule,
  type WatchBoard,
  type WatchRules,
} from "../../api/watch";
import { Callout } from "../../components/Callout";
import { bandRuleLabel, percentBps, referenceLabels } from "./rules";

type Condition = "yield" | "daily" | "weekly";
type BandDraft = Omit<BandRule, "offsetBps"> & { offset: string };
type Draft = { minYield: string; daily: BandDraft; weekly: BandDraft };

const periodLabels = { daily: "日线", weekly: "周线" } as const;

function form(rules: WatchRules): Draft {
  return {
    minYield: String(rules.minYieldBps / 100),
    daily: { ...rules.daily, offset: String(rules.daily.offsetBps / 100) },
    weekly: { ...rules.weekly, offset: String(rules.weekly.offsetBps / 100) },
  };
}

function band(draft: BandDraft): BandRule | null {
  const offsetBps = percentBps(draft.offset);
  return offsetBps == null
    ? null
    : { reference: draft.reference, mode: draft.mode, offsetBps };
}

export function WatchRuleStrip({
  rules,
  defaults,
  disabled,
  onEdited,
  onClosed,
  onSaved,
  onReload,
}: {
  rules: WatchRules;
  defaults?: WatchRules;
  disabled: boolean;
  onEdited: () => void;
  onClosed: () => void;
  onSaved: (board: WatchBoard) => void;
  onReload: (board: WatchBoard) => void;
}) {
  const [editing, setEditing] = useState<Condition | null>(null);
  const [expected, setExpected] = useState(rules);
  const [draft, setDraft] = useState(() => form(rules));
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const open = (condition: Condition) => {
    setEditing(condition);
    setExpected(rules);
    setDraft(form(rules));
    setDirty(false);
    setError("");
  };
  const close = () => {
    if (busy) return;
    if (dirty && !window.confirm("放弃未保存的规则修改？")) return;
    setEditing(null);
    setDirty(false);
    onClosed();
  };
  const change = (next: Draft) => {
    setDraft(next);
    setDirty(true);
    onEdited();
  };
  const save = async () => {
    const minYieldBps = percentBps(draft.minYield);
    const daily = band(draft.daily);
    const weekly = band(draft.weekly);
    if (minYieldBps == null || !daily || !weekly) {
      setError("请输入百分比，最多两位小数。");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const board = await saveWatchRules(
        { minYieldBps, daily, weekly },
        expected,
      );
      setEditing(null);
      setDirty(false);
      onSaved(board);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const reload = async () => {
    if (dirty && !window.confirm("重新读取将放弃当前规则草稿，确认继续？"))
      return;
    setBusy(true);
    setError("");
    try {
      const board = await getWatchBoard();
      setExpected(board.rules);
      setDraft(form(board.rules));
      setDirty(false);
      onReload(board);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const restoreDefault = (condition: Condition) => {
    if (!defaults) return;
    const fallback = form(defaults);
    change(
      condition === "yield"
        ? { ...draft, minYield: fallback.minYield }
        : { ...draft, [condition]: fallback[condition] },
    );
  };

  const bandFields = (period: "daily" | "weekly") => {
    const label = periodLabels[period];
    const rule = draft[period];
    const update = (patch: Partial<BandDraft>) =>
      change({ ...draft, [period]: { ...rule, ...patch } });
    return (
      <>
        <label>
          参考轨
          <select
            aria-label={`${label}参考轨`}
            value={rule.reference}
            onChange={(e) =>
              update({ reference: e.target.value as BandRule["reference"] })
            }
          >
            {Object.entries(referenceLabels).map(([value, text]) => (
              <option key={value} value={value}>
                {text}
              </option>
            ))}
          </select>
        </label>
        <label>
          比较方式
          <select
            aria-label={`${label}比较方式`}
            value={rule.mode}
            onChange={(e) =>
              update({ mode: e.target.value as BandRule["mode"] })
            }
          >
            <option value="near">上下偏离以内</option>
            <option value="at_or_below">不高于</option>
            <option value="at_or_above">不低于</option>
          </select>
        </label>
        <label>
          {rule.mode === "near" ? "允许偏离（%）" : "参考轨偏移（%）"}
          <Input
            aria-label={`${label}偏移（%）`}
            type="number"
            step="0.01"
            min={rule.mode === "near" ? "0" : "-99"}
            max={rule.mode === "near" ? "99" : "100"}
            value={rule.offset}
            onChange={(e) => update({ offset: e.target.value })}
          />
        </label>
      </>
    );
  };

  const editor = (condition: Condition) => (
    <fieldset className="watch-rule-editor" disabled={busy}>
      {error && (
        <Callout tone="danger">
          <span>{error}</span>
          <button type="button" onClick={() => void reload()}>
            重新读取规则
          </button>
        </Callout>
      )}
      {condition === "yield" ? (
        <label>
          最低税前 TTM 股息率（%）
          <Input
            aria-label="最低税前 TTM 股息率（%）"
            type="number"
            min="0.01"
            max="100"
            step="0.01"
            value={draft.minYield}
            onChange={(e) => change({ ...draft, minYield: e.target.value })}
          />
        </label>
      ) : (
        bandFields(condition)
      )}
      <div className="watch-rule-editor-actions">
        {defaults && (
          <Button
            type="text"
            size="small"
            onClick={() => restoreDefault(condition)}
          >
            恢复默认
          </Button>
        )}
        <Button size="small" onClick={close}>
          取消
        </Button>
        <Button
          type="primary"
          size="small"
          loading={busy}
          onClick={() => void save()}
        >
          保存规则
        </Button>
      </div>
    </fieldset>
  );

  const chips: [Condition, string, string][] = [
    ["yield", "①", `TTM 税前股息 ≥${rules.minYieldBps / 100}%`],
    ["daily", "②", `日线${bandRuleLabel(rules.daily)}`],
    ["weekly", "③", `周线${bandRuleLabel(rules.weekly)}`],
  ];
  return (
    <div className="watch-rule-strip" role="group" aria-label="观察规则">
      {chips.map(([condition, marker, label]) => (
        <Popover
          key={condition}
          trigger="click"
          placement="bottomLeft"
          open={editing === condition}
          onOpenChange={(next) => (next ? open(condition) : close())}
          title={
            condition === "yield"
              ? `${marker} 股息门槛`
              : `${marker} ${periodLabels[condition]} BOLL(20,2)`
          }
          content={editor(condition)}
          destroyOnHidden
        >
          <button
            type="button"
            className={`rule-${condition}`}
            disabled={disabled || (editing !== null && editing !== condition)}
            aria-expanded={editing === condition}
            aria-haspopup="dialog"
            title="点击修改"
          >
            {marker} {label}
          </button>
        </Popover>
      ))}
    </div>
  );
}
