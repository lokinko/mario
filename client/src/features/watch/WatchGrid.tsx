import { useEffect, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import {
  previewWatchGrid,
  type GridSizeUnit,
  type WatchGridPlan,
  type WatchGridProjection,
  type WatchRow,
} from "../../api/watch";
import { useDrawerControls } from "../../components/Drawer";
import { formatNumber, formatPercent } from "../../lib/format";
import {
  cents,
  generatePriceLevels,
  gridDividend,
  priceAtYield,
  yieldText,
  type PriceGridGenerator,
} from "./gridDraft";

const money = (amount: string | null | undefined) =>
  amount == null ? "—" : `¥${formatNumber(amount)}`;
const yuan = (price: string) => `¥${formatNumber(price, 2, true)}`;
const shares = (amount: number) => `${formatNumber(amount, 0)} 股`;
const yieldLabel = (dividend: number | null, price: string) => {
  const value = yieldText(dividend, price);
  return value ? `${value}%` : "—";
};

function BudgetShare({ pct }: { pct: string | null }) {
  if (pct == null) return <>—</>;
  const value = Number(pct);
  return (
    <span className={value > 100 ? "grid-budget over" : "grid-budget"}>
      <span className="grid-budget-bar" aria-hidden="true">
        <i style={{ width: `${Math.min(value, 100)}%` }} />
      </span>
      {formatNumber(pct, 1)}%
    </span>
  );
}

function GridScenarios({
  plan,
  result,
}: {
  plan: WatchGridPlan;
  result: WatchGridProjection;
}) {
  if (!result.scenarios.length) return null;
  return (
    <div className="table-scroll">
      <table className="watch-events">
        <caption>跌到各买入档后的测算</caption>
        <thead>
          <tr>
            <th>跌到</th>
            <th>累计投入</th>
            <th>占配置金额</th>
            <th>持仓均价</th>
            <th>回到基准价 {yuan(plan.basePrice)}</th>
            {result.topPrice && <th>涨到 {yuan(result.topPrice)}</th>}
          </tr>
        </thead>
        <tbody>
          {result.scenarios.map((s) => (
            <tr key={s.deepestPrice}>
              <td>{yuan(s.deepestPrice)}</td>
              <td>
                {money(s.investedAmount)}
                <small>{shares(s.boughtShares)}</small>
              </td>
              <td>
                <BudgetShare pct={s.budgetPct} />
              </td>
              <td>¥{formatNumber(s.averageCost, 3)}</td>
              <td>
                {money(s.baseProfit)}
                <small>{formatPercent(s.baseReturnPct)}</small>
              </td>
              {result.topPrice && (
                <td>
                  {money(s.topProfit)}
                  <small>
                    {formatPercent(s.topReturnPct!)}
                    {s.remainingShares > 0 &&
                      ` · 余 ${formatNumber(s.remainingShares, 0)} 股`}
                  </small>
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

const assumptions =
  "按各档价格成交，规模按 100 股向下取整；上涨时最多卖出本轮已买入的股数，未卖出部分按最高卖出档估值。收益率按累计投入计算，未计持有期分红、税费和滑点，不代表价格到达的概率。";

export function WatchGrid({
  row,
  onEdit,
}: {
  row: WatchRow;
  onEdit: () => void;
}) {
  const plan = row.entry.gridPlan;
  const result = row.gridProjection;
  const dividend = gridDividend(row);
  return (
    <section className="watch-detail-section" aria-label="股息网格">
      <div className="watch-section-title">
        <h3>股息网格</h3>
        <button className="text-button" onClick={onEdit}>
          {plan ? "编辑网格方案" : "设置网格方案"}
        </button>
      </div>
      {plan && result && (
        <>
          <p>
            标的配置金额 {money(plan.budget)}，基准价 {yuan(plan.basePrice)}
            {dividend
              ? `（股息率 ${yieldLabel(dividend, plan.basePrice)}）`
              : ""}
            。基准价以下买入，以上卖出。
          </p>
          <div className="table-scroll">
            <table className="watch-events">
              <caption>网格档位</caption>
              <thead>
                <tr>
                  <th>方向</th>
                  <th>价格</th>
                  <th>股息率</th>
                  <th>股数</th>
                  <th>金额</th>
                  <th>累计投入占配置</th>
                </tr>
              </thead>
              <tbody>
                {result.levels.map((level) => (
                  <tr key={level.price}>
                    <td>{level.side === "buy" ? "买入" : "卖出"}</td>
                    <td>{yuan(level.price)}</td>
                    <td>{yieldLabel(dividend, level.price)}</td>
                    <td>{level.shares ? shares(level.shares) : "不足一手"}</td>
                    <td>{money(level.amount)}</td>
                    <td>
                      <BudgetShare pct={level.budgetPct} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <GridScenarios plan={plan} result={result} />
          <p className="watch-meta">{assumptions}</p>
        </>
      )}
    </section>
  );
}

type DraftLevel = { price: string; yieldPct: string; size: string };
const unitLabel = (unit: GridSizeUnit) =>
  unit === "amount" ? "金额（元）" : "占配置 %";

function convertSize(size: string, budget: number, to: GridSizeUnit) {
  const value = Number(size);
  if (!size.trim() || !(budget > 0) || !Number.isFinite(value)) return size;
  const next =
    to === "amount" ? (budget * value) / 100 : (value / budget) * 100;
  return String(Number(next.toFixed(2)));
}

function draftPlan(
  budget: string,
  basePrice: string,
  sizeUnit: GridSizeUnit,
  levels: DraftLevel[],
): WatchGridPlan {
  return {
    budget,
    basePrice,
    sizeUnit,
    levels: levels.map((l) => ({ price: l.price, size: l.size || "0" })),
  };
}

type GridPreview = { result?: WatchGridProjection; error?: string };

function useGridPreview(plan: WatchGridPlan): GridPreview {
  const [state, setState] = useState<GridPreview & { key: string }>({
    key: "",
  });
  const key = JSON.stringify(plan);
  const ready =
    Number(plan.budget) > 0 &&
    cents(plan.basePrice) > 0 &&
    plan.levels.length > 0 &&
    plan.levels.every((l) => cents(l.price) > 0 && l.size.trim() !== "");
  useEffect(() => {
    if (!ready) return;
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      previewWatchGrid(JSON.parse(key) as WatchGridPlan, controller.signal)
        .then((result) => setState({ key, result }))
        .catch((e: Error) => {
          if (!controller.signal.aborted) setState({ key, error: e.message });
        });
    }, 300);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [key, ready]);
  return ready && state.key === key ? state : {};
}

export function WatchGridEditor({
  plan,
  dividend,
  close,
  busy,
  onEdited,
  onCancel,
  onSave,
}: {
  plan: WatchGridPlan | undefined;
  dividend: number | null;
  close: number | null;
  busy: boolean;
  onEdited: () => void;
  onCancel: () => void;
  onSave: (plan: WatchGridPlan | null) => Promise<boolean>;
}) {
  const drawer = useDrawerControls();
  const [budget, setBudget] = useState(plan?.budget ?? "");
  const [basePrice, setBasePrice] = useState(
    plan?.basePrice ?? (close ? close.toFixed(2) : ""),
  );
  const [sizeUnit, setSizeUnit] = useState<GridSizeUnit>(
    plan?.sizeUnit ?? "budgetPct",
  );
  const [levels, setLevels] = useState<DraftLevel[]>(
    () =>
      plan?.levels.map((l) => ({
        price: l.price,
        yieldPct: yieldText(dividend, l.price),
        size: l.size === "0" ? "" : l.size,
      })) ?? [],
  );
  const [generator, setGenerator] = useState<
    Omit<PriceGridGenerator, "basePrice">
  >({
    stepUnit: "pct",
    step: "5",
    down: "4",
    up: "2",
    buySize: "",
    sellSize: "",
  });
  const [error, setError] = useState("");
  const [confirmGenerate, setConfirmGenerate] = useState(false);
  const draft = draftPlan(budget, basePrice, sizeUnit, levels);
  const preview = useGridPreview(draft);
  const projected = (price: string) =>
    preview.result?.levels.find((l) => cents(l.price) === cents(price));
  const edited = () => {
    setError("");
    setConfirmGenerate(false);
    onEdited();
  };
  const updateLevel = (index: number, change: Partial<DraftLevel>) => {
    setLevels(levels.map((l, i) => (i === index ? { ...l, ...change } : l)));
    edited();
  };
  const save = async (value: WatchGridPlan | null) => {
    if (await onSave(value)) drawer.markClean();
  };
  const generate = () => {
    try {
      const next = generatePriceLevels({ ...generator, basePrice });
      if (levels.length && !confirmGenerate) {
        setConfirmGenerate(true);
        return;
      }
      setLevels(
        next.map((l) => ({ ...l, yieldPct: yieldText(dividend, l.price) })),
      );
      edited();
    } catch (e) {
      setError((e as Error).message);
    }
  };
  const base = cents(basePrice);
  const deepest = preview.result?.scenarios.at(-1);
  return (
    <form
      className="watch-editor watch-grid-editor"
      onSubmit={(event) => {
        event.preventDefault();
        const prices = levels.map((l) => cents(l.price));
        if (
          prices.some((p) => !(p > 0) || p === base) ||
          new Set(prices).size !== prices.length
        ) {
          setError("档位价格须互不重复且不等于基准价。");
          return;
        }
        if (!levels.some((l, i) => prices[i] < base && Number(l.size) > 0)) {
          setError("至少在基准价以下设置一档买入规模。");
          return;
        }
        void save(draft);
      }}
    >
      <div className="form-grid">
        <label>
          标的配置金额（元）
          <input
            type="number"
            inputMode="decimal"
            min="0.01"
            max="1000000000000"
            step="0.01"
            required
            value={budget}
            disabled={busy}
            onChange={(e) => {
              const next = e.target.value;
              setBudget(next);
              edited();
            }}
          />
        </label>
        <label>
          基准价（元）
          <input
            type="number"
            inputMode="decimal"
            min="0.01"
            step="0.01"
            required
            value={basePrice}
            disabled={busy}
            onChange={(e) => {
              setBasePrice(e.target.value);
              edited();
            }}
          />
          <small>
            {dividend
              ? `股息率 ${yieldLabel(dividend, basePrice)}`
              : "暂无有效分红，只能按价格设置档位"}
          </small>
        </label>
      </div>
      <details className="watch-grid-generator" open={!plan}>
        <summary>批量生成</summary>
        <fieldset disabled={busy}>
          <div className="form-grid">
            <label>
              档距方式
              <select
                value={generator.stepUnit}
                onChange={(e) => {
                  setGenerator({
                    ...generator,
                    stepUnit: e.target.value as "pct" | "price",
                  });
                  edited();
                }}
              >
                <option value="pct">按比例（%）</option>
                <option value="price">按价差（元）</option>
              </select>
            </label>
            {(
              [
                [
                  "step",
                  generator.stepUnit === "pct"
                    ? "每档相差 %"
                    : "每档相差（元）",
                ],
                ["down", "向下档数"],
                ["up", "向上档数"],
                ["buySize", `每档买入 ${unitLabel(sizeUnit)}`],
                ["sellSize", `每档卖出 ${unitLabel(sizeUnit)}`],
              ] as const
            ).map(([key, label]) => (
              <label key={key}>
                {label}
                <input
                  type="number"
                  inputMode="decimal"
                  min="0"
                  step={key === "down" || key === "up" ? 1 : 0.01}
                  value={generator[key]}
                  onChange={(e) => {
                    setGenerator({ ...generator, [key]: e.target.value });
                    edited();
                  }}
                />
              </label>
            ))}
          </div>
          <button className="secondary" type="button" onClick={generate}>
            {confirmGenerate ? "确认替换现有档位" : "生成网格"}
          </button>
          {confirmGenerate && (
            <p role="status">生成将替换当前档位；请再次点击确认替换。</p>
          )}
        </fieldset>
      </details>
      {error && <p role="alert">{error}</p>}
      <div className="watch-grid-toolbar">
        <label>
          规模单位
          <select
            value={sizeUnit}
            disabled={busy}
            onChange={(e) => {
              const next = e.target.value as GridSizeUnit;
              setLevels(
                levels.map((l) => ({
                  ...l,
                  size: convertSize(l.size, Number(budget), next),
                })),
              );
              setSizeUnit(next);
              edited();
            }}
          >
            <option value="budgetPct">占配置金额 %</option>
            <option value="amount">金额（元）</option>
          </select>
        </label>
        {deepest && (
          <span role="status">
            全部买入档累计投入 {money(deepest.investedAmount)}，占配置{" "}
            <BudgetShare pct={deepest.budgetPct} />
          </span>
        )}
        {preview.error && <span className="danger-text">{preview.error}</span>}
      </div>
      <div className="table-scroll">
        <table className="watch-events watch-grid-inputs">
          <thead>
            <tr>
              <th>方向</th>
              <th>价格（元）</th>
              <th>股息率 %</th>
              <th>{unitLabel(sizeUnit)}</th>
              <th>股数</th>
              <th>金额</th>
              <th>累计投入占配置</th>
              <th>
                <span className="sr-only">操作</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {levels.map((level, index) => {
              const price = cents(level.price);
              const side =
                !(price > 0) || !(base > 0) || price === base
                  ? "—"
                  : price < base
                    ? "买入"
                    : "卖出";
              const result = projected(level.price);
              return (
                <tr key={index}>
                  <td>{side}</td>
                  <td>
                    <input
                      type="number"
                      inputMode="decimal"
                      min="0.01"
                      step="0.01"
                      required
                      aria-label={`第 ${index + 1} 档价格`}
                      value={level.price}
                      disabled={busy}
                      onChange={(e) =>
                        updateLevel(index, {
                          price: e.target.value,
                          yieldPct: yieldText(dividend, e.target.value),
                        })
                      }
                    />
                  </td>
                  <td>
                    <input
                      type="number"
                      inputMode="decimal"
                      min="0.01"
                      max="100"
                      step="0.01"
                      aria-label={`第 ${index + 1} 档股息率`}
                      value={level.yieldPct}
                      disabled={busy || !dividend}
                      onChange={(e) =>
                        updateLevel(index, {
                          yieldPct: e.target.value,
                          price:
                            priceAtYield(dividend, e.target.value) ||
                            level.price,
                        })
                      }
                    />
                  </td>
                  <td>
                    <input
                      type="number"
                      inputMode="decimal"
                      min="0"
                      step="0.01"
                      aria-label={`第 ${index + 1} 档规模`}
                      value={level.size}
                      disabled={busy}
                      onChange={(e) =>
                        updateLevel(index, { size: e.target.value })
                      }
                    />
                  </td>
                  <td>
                    {!result
                      ? "—"
                      : result.shares
                        ? shares(result.shares)
                        : Number(level.size) > 0
                          ? "不足一手"
                          : "—"}
                  </td>
                  <td>{money(result?.amount)}</td>
                  <td>
                    <BudgetShare pct={result?.budgetPct ?? null} />
                  </td>
                  <td>
                    <button
                      type="button"
                      className="icon-button danger-text"
                      aria-label="删除"
                      title="删除"
                      disabled={busy}
                      onClick={() => {
                        setLevels(levels.filter((_, i) => i !== index));
                        edited();
                      }}
                    >
                      <Trash2 size={15} />
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {!levels.length && (
        <p className="watch-meta">通过批量生成或添加档位开始设置网格。</p>
      )}
      <button
        type="button"
        className="secondary"
        disabled={busy || levels.length >= 100}
        onClick={() => {
          setLevels([...levels, { price: "", yieldPct: "", size: "" }]);
          edited();
        }}
      >
        <Plus size={16} />
        添加
      </button>
      {preview.result && (
        <>
          <GridScenarios plan={draft} result={preview.result} />
          <p className="watch-meta">{assumptions}</p>
        </>
      )}
      <div className="form-actions">
        {plan && (
          <button
            className="text-button danger"
            type="button"
            disabled={busy}
            onClick={() => {
              if (window.confirm("清除该公司的网格方案？")) void save(null);
            }}
          >
            清除网格方案
          </button>
        )}
        <button
          className="secondary"
          type="button"
          disabled={busy}
          onClick={() => {
            if (!drawer.confirmDiscard()) return;
            drawer.markClean();
            onCancel();
          }}
        >
          取消
        </button>
        <button className="primary" type="submit" disabled={busy}>
          保存
        </button>
      </div>
    </form>
  );
}
