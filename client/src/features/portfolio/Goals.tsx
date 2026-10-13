import { useEffect, useRef, useState } from "react";
import { Edit3, Plus, Save, Trash2 } from "lucide-react";
import { deleteGoal, saveGoal, updateGoal } from "../../api/portfolio";
import { NumberField } from "../../components/NumberField";
import { PageHeader } from "../../components/PageHeader";
import { formatMoney } from "../../lib/format";
import type { Goal, Snapshot } from "../../types";
import { emptyGoal } from "./forms";

export function Goals({
  snapshot,
  onUpdate,
  flash,
}: {
  snapshot: Snapshot;
  onUpdate: (snapshot: Snapshot) => void;
  flash: (message: string) => void;
}) {
  const [draft, setDraft] = useState<Omit<Goal, "id"> | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const nameInput = useRef<HTMLInputElement>(null);
  const currency = snapshot.profile.baseCurrency;
  useEffect(() => {
    if (draft) nameInput.current?.focus();
  }, [Boolean(draft), editingId]);
  const valid = Boolean(
    draft &&
    draft.name.trim() &&
    draft.targetDate &&
    Number.isFinite(draft.targetAmount) &&
    draft.targetAmount > 0 &&
    draft.targetAmount <= 1e15 &&
    [draft.currentAmount, draft.monthlyContribution].every(
      (value) => Number.isFinite(value) && value >= 0 && value <= 1e15,
    ),
  );
  const close = () => {
    setDraft(null);
    setEditingId(null);
    setError("");
  };
  const persist = async () => {
    if (busy || !draft || !valid) return;
    setBusy(true);
    setError("");
    try {
      const input = { ...draft, name: draft.name.trim() };
      onUpdate(
        editingId ? await updateGoal(editingId, input) : await saveGoal(input),
      );
      close();
      flash(editingId ? "投资目标已更新" : "投资目标已添加");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusy(false);
    }
  };
  const remove = async (goal: Goal) => {
    if (busy || !window.confirm(`确认删除目标“${goal.name}”？`)) return;
    setBusy(true);
    setError("");
    try {
      onUpdate(await deleteGoal(goal.id));
      if (editingId === goal.id) close();
      flash("投资目标已删除");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="page narrow goals-page">
      <PageHeader
        title="投资目标"
        description="记录想实现的计划，以及你准备投入的金额。"
        action={
          <button
            className="primary"
            disabled={busy || Boolean(draft)}
            onClick={() => {
              setEditingId(null);
              setDraft({ ...emptyGoal });
              setError("");
            }}
          >
            <Plus size={16} />
            添加目标
          </button>
        }
      />
      {error && (
        <div className="error-box" role="alert">
          {error}。输入已保留，请核实后重试。
        </div>
      )}
      {draft && (
        <section
          className="panel form-panel"
          aria-label={editingId ? "编辑目标" : "添加目标"}
        >
          <h2>{editingId ? "编辑目标" : "添加目标"}</h2>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void persist();
            }}
          >
            <fieldset className="holding-fields" disabled={busy}>
              <div className="form-grid">
                <label>
                  <span>目标名称</span>
                  <input
                    ref={nameInput}
                    required
                    value={draft.name}
                    onChange={(event) =>
                      setDraft({ ...draft, name: event.target.value })
                    }
                    placeholder="例如：养老储备、购房首付"
                  />
                </label>
                <label>
                  <span>目标日期</span>
                  <input
                    required
                    type="date"
                    value={draft.targetDate}
                    onChange={(event) =>
                      setDraft({ ...draft, targetDate: event.target.value })
                    }
                  />
                </label>
                <NumberField
                  label="目标金额"
                  value={draft.targetAmount}
                  prefix={currency}
                  onChange={(value) =>
                    setDraft({ ...draft, targetAmount: Number(value) })
                  }
                />
                <NumberField
                  label="已经投入"
                  value={draft.currentAmount}
                  prefix={currency}
                  onChange={(value) =>
                    setDraft({ ...draft, currentAmount: Number(value) })
                  }
                />
                <NumberField
                  label="计划每月投入"
                  value={draft.monthlyContribution}
                  prefix={currency}
                  onChange={(value) =>
                    setDraft({ ...draft, monthlyContribution: Number(value) })
                  }
                />
                <label>
                  <span>目标优先级</span>
                  <select
                    value={draft.priority}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        priority: event.target.value as Goal["priority"],
                      })
                    }
                  >
                    <option>刚性</option>
                    <option>重要</option>
                    <option>弹性</option>
                  </select>
                </label>
              </div>
              <div className="form-actions">
                <button type="button" className="text-button" onClick={close}>
                  取消
                </button>
                <button type="submit" className="primary" disabled={!valid}>
                  <Save size={16} />
                  {busy ? "保存中…" : "保存目标"}
                </button>
              </div>
            </fieldset>
          </form>
        </section>
      )}
      <div className="goal-cards">
        {snapshot.goals.map((goal) => {
          const progress = Math.min(
            100,
            Math.max(0, (goal.currentAmount / goal.targetAmount) * 100),
          );
          return (
            <article className="panel goal-card" key={goal.id}>
              <div className="panel-title">
                <div>
                  <small>
                    {goal.priority} · {goal.targetDate}
                  </small>
                  <h2>{goal.name}</h2>
                </div>
                <div className="row-actions">
                  <button
                    disabled={busy || Boolean(draft)}
                    aria-label={`编辑目标 ${goal.name}`}
                    onClick={() => {
                      const { id, ...values } = goal;
                      setEditingId(id);
                      setDraft(values);
                      setError("");
                    }}
                  >
                    <Edit3 size={16} />
                  </button>
                  <button
                    disabled={busy || Boolean(draft)}
                    aria-label={`删除目标 ${goal.name}`}
                    onClick={() => void remove(goal)}
                  >
                    <Trash2 size={16} />
                  </button>
                </div>
              </div>
              <strong className="goal-amount">
                {formatMoney(goal.currentAmount, currency)}
                <small> / {formatMoney(goal.targetAmount, currency)}</small>
              </strong>
              <progress
                max={100}
                value={progress}
                aria-label={`${goal.name} 已投入比例`}
              />
              <p className="goal-detail">
                已投入 {Math.round(progress)}% · 每月计划{" "}
                {formatMoney(goal.monthlyContribution, currency)}
              </p>
            </article>
          );
        })}
      </div>
      {!snapshot.goals.length && !draft && (
        <section className="panel empty-state">
          <h2>想为哪件事做准备？</h2>
          <p>添加一个目标，记录金额、日期和每月投入计划。</p>
          <button
            className="secondary"
            onClick={() => setDraft({ ...emptyGoal })}
          >
            <Plus size={16} />
            创建第一个目标
          </button>
        </section>
      )}
    </div>
  );
}
