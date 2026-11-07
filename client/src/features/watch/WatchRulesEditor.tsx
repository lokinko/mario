import { useState } from "react";
import { Button, Input } from "antd";
import {
  getWatchBoard,
  saveWatchRules,
  type BandRule,
  type WatchBoard,
  type WatchRules,
} from "../../api/watch";
import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { percentBps, referenceLabels } from "./rules";

function form(rules: WatchRules) {
  return {
    minYield: String(rules.minYieldBps / 100),
    daily: { ...rules.daily, offset: String(rules.daily.offsetBps / 100) },
    weekly: { ...rules.weekly, offset: String(rules.weekly.offsetBps / 100) },
  };
}
export function WatchRulesEditor({
  rules,
  defaults,
  onClose,
  onEdited,
  onSaved,
  onReload,
}: {
  rules: WatchRules;
  defaults: WatchRules;
  onClose: () => void;
  onEdited: () => void;
  onSaved: (board: WatchBoard) => void;
  onReload: (board: WatchBoard) => void;
}) {
  const [expected, setExpected] = useState(rules);
  const [draft, setDraft] = useState(() => form(rules));
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const change = (next: typeof draft) => {
    setDraft(next);
    setDirty(true);
    onEdited();
  };
  const save = async () => {
    const minYieldBps = percentBps(draft.minYield);
    const daily = percentBps(draft.daily.offset);
    const weekly = percentBps(draft.weekly.offset);
    if (minYieldBps == null || daily == null || weekly == null) {
      setError("请输入百分比，最多两位小数。");
      return;
    }
    setBusy(true);
    setError("");
    try {
      onSaved(
        await saveWatchRules(
          {
            minYieldBps,
            daily: {
              reference: draft.daily.reference,
              mode: draft.daily.mode,
              offsetBps: daily,
            },
            weekly: {
              reference: draft.weekly.reference,
              mode: draft.weekly.mode,
              offsetBps: weekly,
            },
          },
          expected,
        ),
      );
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
  return (
    <Drawer
      title="观察规则"
      onClose={onClose}
      busy={busy}
      dirty={dirty}
      resetKey={expected}
    >
      <div className="watch-editor watch-rules-editor">
        <p>
          本账号的自选与行业列表共用。保存后用已有行情立即重算当前结果；历史记录保留当时规则，修改本身不产生进出提醒。
        </p>
        {error && <Callout tone="danger">{error}</Callout>}
        <label>
          最低税前 TTM 股息率（%）
          <Input
            aria-label="最低税前 TTM 股息率（%）"
            type="number"
            min="0.01"
            max="100"
            step="0.01"
            disabled={busy}
            value={draft.minYield}
            onChange={(e) => change({ ...draft, minYield: e.target.value })}
          />
        </label>
        {(["daily", "weekly"] as const).map((period) => {
          const label = period === "daily" ? "日线" : "周线";
          const rule = draft[period];
          const update = (patch: Partial<typeof rule>) =>
            change({ ...draft, [period]: { ...rule, ...patch } });
          return (
            <fieldset key={period} disabled={busy}>
              <legend>{label} BOLL(20,2)</legend>
              <label>
                {label}参考轨
                <select
                  aria-label={`${label}参考轨`}
                  value={rule.reference}
                  onChange={(e) =>
                    update({
                      reference: e.target.value as BandRule["reference"],
                    })
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
                {label}比较方式
                <select
                  aria-label={`${label}比较方式`}
                  value={rule.mode}
                  onChange={(e) =>
                    update({ mode: e.target.value as BandRule["mode"] })
                  }
                >
                  <option value="near">附近（上下偏离）</option>
                  <option value="at_or_below">不高于</option>
                  <option value="at_or_above">不低于</option>
                </select>
              </label>
              <label>
                {label}
                {rule.mode === "near" ? "允许偏离" : "参考轨偏移"}（%）
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
              <p className="watch-meta">
                {rule.mode === "near"
                  ? "收盘价位于所选轨道价格上下该百分比以内，包含边界。"
                  : "先将参考轨价格乘以（1 + 偏移百分比），再与收盘价比较，包含相等；负值向下偏移。"}
              </p>
            </fieldset>
          );
        })}
        <div className="watch-actions">
          <Button type="primary" disabled={busy} onClick={() => void save()}>
            保存观察规则
          </Button>
          <Button disabled={busy} onClick={() => change(form(defaults))}>
            恢复默认值
          </Button>
          <Button disabled={busy} onClick={() => void reload()}>
            重新读取规则
          </Button>
        </div>
      </div>
    </Drawer>
  );
}
