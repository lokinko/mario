import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import type {
  WatchGridGenerator,
  WatchGridPlan,
  WatchRow,
} from "../../api/watch";
import { useDrawerControls } from "../../components/Drawer";
import { formatNumber, formatPercent } from "../../lib/format";
import { generateGridLevels, maxShares, yieldLabel } from "./gridDraft";

const reference = (price: string | null | undefined) =>
  price == null ? "—" : `约 ¥${formatNumber(price, 4)}`;
const money = (amount: string | null | undefined) =>
  amount == null ? "—" : `¥${formatNumber(amount)}`;
const shares = (amount: number) => `${formatNumber(amount, 0)} 股`;

export function WatchGrid({
  row,
  onEdit,
}: {
  row: WatchRow;
  onEdit: () => void;
}) {
  const plan = row.entry.gridPlan;
  const result = row.gridProjection;
  const archive = row.entry.previousGridPlan;
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
            标的配置金额 {money(plan.budget)}
            。股价下跌、股息率上升时买入；股价上涨、股息率下降时卖出，各档均按填写股数测算。
          </p>
          <div className="table-scroll">
            <table className="watch-events">
              <caption>网格档位</caption>
              <thead>
                <tr>
                  <th>股息率</th>
                  <th>参考价</th>
                  <th>下跌买入</th>
                  <th>预计投入</th>
                  <th>上涨卖出</th>
                  <th>预计回款</th>
                </tr>
              </thead>
              <tbody>
                {result.levels.map((point) => {
                  const level = plan.levels.find(
                    (item) => item.yieldBps === point.yieldBps,
                  )!;
                  return (
                    <tr key={point.yieldBps}>
                      <td>{yieldLabel(point.yieldBps)}</td>
                      <td>{reference(point.price)}</td>
                      <td>{shares(level.buyShares)}</td>
                      <td>{money(point.buyAmount)}</td>
                      <td>{shares(level.sellShares)}</td>
                      <td>{money(point.sellAmount)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {result.scenarios.length > 0 && (
            <div className="table-scroll">
              <table className="watch-events">
                <caption>
                  跌到各档后回到 {yieldLabel(result.reboundYieldBps!)}{" "}
                  的价差测算
                </caption>
                <thead>
                  <tr>
                    <th>最深触及</th>
                    <th>累计投入</th>
                    <th>预算余额／缺口</th>
                    <th>已卖股数</th>
                    <th>剩余股数</th>
                    <th>卖出不足股数</th>
                    <th>卖出回款</th>
                    <th>含余股估值盈亏</th>
                    <th>按已投入算</th>
                    <th>按总预算算</th>
                  </tr>
                </thead>
                <tbody>
                  {result.scenarios.map((s) => (
                    <tr key={s.deepestYieldBps}>
                      <td>{yieldLabel(s.deepestYieldBps)}</td>
                      <td>{money(s.investedAmount)}</td>
                      <td>{money(s.budgetGap)}</td>
                      <td>{shares(s.soldShares)}</td>
                      <td>{shares(s.remainingShares)}</td>
                      <td>{shares(s.unfilledSellShares)}</td>
                      <td>{money(s.saleProceeds)}</td>
                      <td>{money(s.profit)}</td>
                      <td>{formatPercent(s.investedReturnPct)}</td>
                      <td>{formatPercent(s.budgetReturnPct)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <p className="watch-meta">
            假设 TTM
            每股分红不变，下跌与上涨各经过一档操作一次，反弹只卖出比最深触及更低的股息率档位。
            预算负数表示计划资金缺口；卖出股数最多取本轮已买余股，超出部分列为卖出不足股数。
            剩余股数按最低股息率档参考价估值，不自动清仓。盈亏未计分红、税费、滑点和整手限制，不代表反弹概率。
            {result.scenarios.length
              ? `参考价采用 ${row.latest?.observedOn} 的已实施分红口径。`
              : "暂无有效的正分红，保留计划股数，暂不测算价格、用款和收益。"}
          </p>
        </>
      )}
      {archive && (
        <details>
          <summary>旧比例方案（历史参考）</summary>
          <p className="watch-meta">
            旧方案未填写配置金额，不能直接换算股数。请通过
            {plan ? "编辑" : "设置"}
            网格方案填写金额和股数。此记录仅供核对，不参与测算。
          </p>
          <p>
            买入比例按
            {archive.buyBasis === "budget" ? "计划总资金" : "触发时剩余现金"}
            ，减仓比例按
            {archive.sellBasis === "totalBought"
              ? "本轮累计买入股数"
              : "触发时剩余股数"}
            ；原方案回到 4% 卖完剩余份额。
          </p>
          <ul>
            {archive.levels.map((l) => (
              <li key={l.yieldBps}>
                {yieldLabel(l.yieldBps)} · 买入 {l.buyBps / 100}% · 减仓{" "}
                {l.sellBps / 100}%
              </li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}

const generatorDefaults = {
  startYieldBps: 400,
  endYieldBps: 600,
  stepBps: 50,
  initialBuyShares: 0,
  initialSellShares: 0,
  buyGrowthBps: 0,
  sellGrowthBps: 0,
};
const generatorFields = [
  ["startYieldBps", "最低股息率 %", 100, 0.01, 100, 0.01],
  ["endYieldBps", "最高股息率 %", 100, 0.01, 100, 0.01],
  ["stepBps", "网格间距（百分点）", 100, 0.01, 100, 0.01],
  ["initialBuyShares", "首档买入股数", 1, 0, maxShares, 1],
  ["initialSellShares", "首档卖出股数", 1, 0, maxShares, 1],
  ["buyGrowthBps", "每档买入股数递增 %", 100, -100, 1000, 0.01],
  ["sellGrowthBps", "每档卖出股数递增 %", 100, -100, 1000, 0.01],
] as const;

export function WatchGridEditor({
  plan,
  busy,
  onEdited,
  onCancel,
  onSave,
}: {
  plan: WatchGridPlan | undefined;
  busy: boolean;
  onEdited: () => void;
  onCancel: () => void;
  onSave: (plan: WatchGridPlan | null) => Promise<boolean>;
}) {
  const drawer = useDrawerControls();
  const [budget, setBudget] = useState(plan?.budget ?? "");
  const [generator, setGenerator] = useState(
    () =>
      Object.fromEntries(
        generatorFields.map(([key, , divisor]) => [
          key,
          String((plan?.generator ?? generatorDefaults)[key] / divisor),
        ]),
      ) as Record<keyof WatchGridGenerator, string>,
  );
  const [generatedWith, setGeneratedWith] = useState(plan?.generator);
  const [levels, setLevels] = useState(() =>
    (
      plan?.levels ??
      [400, 450, 500, 550, 600].map((yieldBps) => ({
        yieldBps,
        buyShares: 0,
        sellShares: 0,
      }))
    ).map((l) => ({
      yieldPct: String(l.yieldBps / 100),
      buy: l.buyShares ? String(l.buyShares) : "",
      sell: l.sellShares ? String(l.sellShares) : "",
    })),
  );
  const [error, setError] = useState("");
  const [confirmGenerate, setConfirmGenerate] = useState(false);
  const edited = () => {
    setError("");
    setConfirmGenerate(false);
    onEdited();
  };
  const save = async (value: WatchGridPlan | null) => {
    if (await onSave(value)) drawer.markClean();
  };
  const generate = () => {
    try {
      const g = Object.fromEntries(
        generatorFields.map(([key, , divisor]) => [
          key,
          Number(generator[key]) * divisor,
        ]),
      ) as unknown as WatchGridGenerator;
      // Decimal percent text has at most two places; remove binary floating point noise.
      for (const key of Object.keys(g) as (keyof WatchGridGenerator)[])
        g[key] = Number(g[key].toFixed(8));
      const next = generateGridLevels(g);
      if (
        levels.some((l) => Number(l.buy) || Number(l.sell)) &&
        !confirmGenerate
      ) {
        setConfirmGenerate(true);
        return;
      }
      setLevels(
        next.map((l) => ({
          yieldPct: String(l.yieldBps / 100),
          buy: String(l.buyShares),
          sell: String(l.sellShares),
        })),
      );
      setGeneratedWith(g);
      edited();
    } catch (e) {
      setError((e as Error).message);
    }
  };
  return (
    <form
      className="watch-editor watch-grid-editor"
      onSubmit={(event) => {
        event.preventDefault();
        const values = levels.map((l) => ({
          yieldBps: Number((Number(l.yieldPct) * 100).toFixed(8)),
          buyShares: Number(l.buy),
          sellShares: Number(l.sell),
        }));
        if (
          new Set(values.map((l) => l.yieldBps)).size !== values.length ||
          !values.some((l) => l.buyShares > 0)
        ) {
          setError("档位不能重复，且至少填写一档买入股数。");
          return;
        }
        void save({
          budget,
          ...(generatedWith ? { generator: generatedWith } : {}),
          levels: values,
        });
      }}
    >
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
            setBudget(e.target.value);
            edited();
          }}
        />
      </label>
      <p>
        股价下跌、股息率上升时买入；股价上涨、股息率下降时卖出。买卖数量均以股为单位，留空表示不操作。
      </p>
      <fieldset disabled={busy}>
        <legend>批量生成网格</legend>
        <div className="form-grid">
          {generatorFields.map(([key, label, , min, max, step]) => (
            <label key={key}>
              {label}
              <input
                type="number"
                inputMode="decimal"
                min={min}
                max={max}
                step={step}
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
          <p role="status">生成将替换当前档位和股数；请再次点击确认替换。</p>
        )}
      </fieldset>
      {error && <p role="alert">{error}</p>}
      <div className="table-scroll">
        <table className="watch-events watch-grid-inputs">
          <thead>
            <tr>
              <th>股息率 %</th>
              <th>下跌买入（股）</th>
              <th>上涨卖出（股）</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {levels.map((level, index) => (
              <tr key={index}>
                <td>
                  <input
                    type="number"
                    min="0.01"
                    max="100"
                    step="0.01"
                    required
                    aria-label={`第 ${index + 1} 档股息率`}
                    value={level.yieldPct}
                    disabled={busy}
                    onChange={(e) => {
                      setLevels(
                        levels.map((l, i) =>
                          i === index ? { ...l, yieldPct: e.target.value } : l,
                        ),
                      );
                      edited();
                    }}
                  />
                </td>
                {(["buy", "sell"] as const).map((key) => (
                  <td key={key}>
                    <input
                      type="number"
                      inputMode="numeric"
                      min="0"
                      max={maxShares}
                      step="1"
                      aria-label={`${yieldLabel(Number(level.yieldPct) * 100)} ${key === "buy" ? "下跌买入" : "上涨卖出"}股数`}
                      value={level[key]}
                      disabled={busy}
                      onChange={(e) => {
                        setLevels(
                          levels.map((l, i) =>
                            i === index ? { ...l, [key]: e.target.value } : l,
                          ),
                        );
                        edited();
                      }}
                    />
                  </td>
                ))}
                <td>
                  <button
                    type="button"
                    className="icon-button danger-text"
                    aria-label={`删除第 ${index + 1} 档`}
                    title="删除"
                    disabled={busy || levels.length === 1}
                    onClick={() => {
                      setLevels(levels.filter((_, i) => i !== index));
                      edited();
                    }}
                  >
                    <Trash2 size={15} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <button
        type="button"
        className="secondary"
        disabled={busy || levels.length >= 100}
        onClick={() => {
          setLevels([...levels, { yieldPct: "", buy: "", sell: "" }]);
          edited();
        }}
      >
        <Plus size={16} />
        添加
      </button>
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
          保存并测算
        </button>
      </div>
    </form>
  );
}
