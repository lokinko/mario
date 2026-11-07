import { useState } from "react";
import type { WatchGridPlan, WatchRow } from "../../api/watch";
import { useDrawerControls } from "../../components/Drawer";
import { formatNumber, formatPercent } from "../../lib/format";

const defaultLevels = [400, 450, 500, 550, 600];
const allLevels = Array.from({ length: 21 }, (_, i) => 400 + i * 10);
const yieldLabel = (value: number) => `${(value / 100).toFixed(1)}%`;
const reference = (price: string | null | undefined) =>
  price == null ? "—" : `约 ¥${formatNumber(price, 4)}`;
const buyBasisLabels = {
  budget: "计划总资金",
  remainingCash: "触发时剩余现金",
};
const sellBasisLabels = {
  totalBought: "本轮累计买入股数",
  remainingShares: "触发时剩余股数",
};

export function WatchGrid({
  row,
  onEdit,
}: {
  row: WatchRow;
  onEdit: () => void;
}) {
  const plan = row.entry.gridPlan;
  const result = row.gridProjection;
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
            买入比例按{buyBasisLabels[plan.buyBasis]}，减仓比例按
            {sellBasisLabels[plan.sellBasis]}。回到 4.0% 卖完本轮剩余份额，
            参考价 {reference(result.exitPrice)}。
          </p>
          <div className="table-scroll">
            <table className="watch-events">
              <caption>网格档位</caption>
              <thead>
                <tr>
                  <th>股息率</th>
                  <th>参考价</th>
                  <th>下跌投入</th>
                  <th>反弹减仓</th>
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
                      <td>{level.buyBps ? `${level.buyBps / 100}%` : "—"}</td>
                      <td>
                        {point.yieldBps === 400
                          ? "卖完剩余份额"
                          : level.sellBps
                            ? `${level.sellBps / 100}%`
                            : "—"}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <div className="table-scroll">
            <table className="watch-events">
              <caption>跌到各档后回到 4.0% 的价差测算</caption>
              <thead>
                <tr>
                  <th>最深触及</th>
                  <th>已投入预算</th>
                  <th>在 4.0% 卖出</th>
                  <th>按已投入算</th>
                  <th>按总预算算</th>
                </tr>
              </thead>
              <tbody>
                {result.scenarios.map((scenario) => (
                  <tr key={scenario.deepestYieldBps}>
                    <td>{yieldLabel(scenario.deepestYieldBps)}</td>
                    <td>{formatNumber(scenario.investedPct)}%</td>
                    <td>{formatNumber(scenario.exitSharesPct)}% 股数</td>
                    <td>{formatPercent(scenario.investedReturnPct)}</td>
                    <td>{formatPercent(scenario.budgetReturnPct)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p className="watch-meta">
            假设 TTM 每股分红不变、下跌和反弹分别按档成交一次。收益只含价差，
            未计持有期分红、税费和滑点，也未估计反弹概率。
            {result.exitPrice
              ? `参考价采用 ${row.latest?.observedOn} 的已实施分红口径。`
              : "暂无有效的正分红，暂不显示参考价。"}
          </p>
        </>
      )}
    </section>
  );
}

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
  const [buyBasis, setBuyBasis] = useState<WatchGridPlan["buyBasis"]>(
    plan?.buyBasis ?? "budget",
  );
  const [sellBasis, setSellBasis] = useState<WatchGridPlan["sellBasis"]>(
    plan?.sellBasis ?? "totalBought",
  );
  const [levels, setLevels] = useState<
    Record<number, { buy: string; sell: string }>
  >(() => ({
    ...Object.fromEntries(
      defaultLevels.map((value) => [value, { buy: "", sell: "" }]),
    ),
    ...Object.fromEntries(
      (plan?.levels ?? []).map((level) => [
        level.yieldBps,
        {
          buy: level.buyBps ? String(level.buyBps / 100) : "",
          sell: level.sellBps ? String(level.sellBps / 100) : "",
        },
      ]),
    ),
  }));
  const ids = Object.keys(levels)
    .map(Number)
    .sort((a, b) => a - b);
  const save = async (value: WatchGridPlan | null) => {
    if (await onSave(value)) drawer.markClean();
  };
  return (
    <form
      className="watch-editor watch-grid-editor"
      onSubmit={(event) => {
        event.preventDefault();
        void save({
          buyBasis,
          sellBasis,
          levels: ids
            .map((yieldBps) => ({
              yieldBps,
              buyBps: Math.round(Number(levels[yieldBps].buy) * 100),
              sellBps: Math.round(Number(levels[yieldBps].sell) * 100),
            }))
            .filter((level) => level.buyBps || level.sellBps),
        });
      }}
    >
      <div className="form-grid">
        <label>
          每档买入占
          <select
            value={buyBasis}
            disabled={busy}
            onChange={(e) => {
              setBuyBasis(e.target.value as WatchGridPlan["buyBasis"]);
              onEdited();
            }}
          >
            <option value="budget">计划总资金</option>
            <option value="remainingCash">触发时剩余现金</option>
          </select>
        </label>
        <label>
          每档减仓占
          <select
            value={sellBasis}
            disabled={busy}
            onChange={(e) => {
              setSellBasis(e.target.value as WatchGridPlan["sellBasis"]);
              onEdited();
            }}
          >
            <option value="totalBought">本轮累计买入股数</option>
            <option value="remainingShares">触发时剩余股数</option>
          </select>
        </label>
      </div>
      <div className="table-scroll">
        <table className="watch-events watch-grid-inputs">
          <thead>
            <tr>
              <th>股息率</th>
              <th>下跌投入 %</th>
              <th>反弹减仓 %</th>
            </tr>
          </thead>
          <tbody>
            {ids.map((id) => (
              <tr key={id}>
                <th scope="row">{yieldLabel(id)}</th>
                {(["buy", "sell"] as const).map((key) => (
                  <td key={key}>
                    {key === "sell" && (id === 400 || id === 600) ? (
                      id === 400 ? (
                        "卖完剩余份额"
                      ) : (
                        "—"
                      )
                    ) : (
                      <input
                        type="number"
                        inputMode="decimal"
                        min="0"
                        max="100"
                        step="0.01"
                        aria-label={`${yieldLabel(id)} ${key === "buy" ? "下跌投入" : "反弹减仓"}比例`}
                        value={levels[id][key]}
                        disabled={busy}
                        onChange={(e) => {
                          setLevels({
                            ...levels,
                            [id]: { ...levels[id], [key]: e.target.value },
                          });
                          onEdited();
                        }}
                      />
                    )}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {ids.length < allLevels.length && (
        <label>
          添加股息率档位
          <select
            value=""
            disabled={busy}
            onChange={(e) => {
              setLevels({
                ...levels,
                [Number(e.target.value)]: { buy: "", sell: "" },
              });
              onEdited();
            }}
          >
            <option value="" disabled>
              选择 4.0% 至 6.0% 内的档位
            </option>
            {allLevels
              .filter((id) => !levels[id])
              .map((id) => (
                <option key={id} value={id}>
                  {yieldLabel(id)}
                </option>
              ))}
          </select>
        </label>
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
          保存并测算
        </button>
      </div>
    </form>
  );
}
