import { useState } from "react";
import type { Snapshot } from "../../types";
import { formatMoney } from "../../lib/format";
import { simulateShock } from "./analytics";

export function ScenarioAnalysis({ snapshot }: { snapshot: Snapshot }) {
  const [marketDrop, setMarketDrop] = useState(20);
  const [fxDrop, setFxDrop] = useState(5);
  const result = simulateShock(snapshot, marketDrop, fxDrop);
  const available = result != null && snapshot.totalValue > 0;
  const money = (value: number | undefined) =>
    available && value != null
      ? formatMoney(value, snapshot.profile.baseCurrency)
      : "—";
  return (
    <section className="panel dashboard-section" aria-label="假设情景分析">
      <div className="panel-title">
        <h2>如果资产下跌，会影响多少？</h2>
        <span>假设情景 · 可调整</span>
      </div>
      <div className="dashboard-scenarios">
        <label>
          <span>
            所有非现金投资市值下跌 <b>{marketDrop}%</b>
          </span>
          <input
            aria-label="投资市值下跌百分比"
            type="range"
            min={0}
            max={60}
            step={1}
            value={marketDrop}
            disabled={!available}
            onChange={(event) => setMarketDrop(Number(event.target.value))}
          />
        </label>
        <label>
          <span>
            外币相对基准币种贬值 <b>{fxDrop}%</b>
          </span>
          <input
            aria-label="外币贬值百分比"
            type="range"
            min={0}
            max={30}
            step={1}
            value={fxDrop}
            disabled={!available}
            onChange={(event) => setFxDrop(Number(event.target.value))}
          />
        </label>
      </div>
      <div className="dashboard-stat-grid">
        <div>
          <span>假设资产减少</span>
          <strong>{money(result?.loss)}</strong>
        </div>
        <div>
          <span>总资产降幅</span>
          <strong>
            {available && result.lossPct != null
              ? `${result.lossPct.toFixed(1)}%`
              : "—"}
          </strong>
        </div>
        <div>
          <span>情景下净资产</span>
          <strong>{money(result?.netAssets)}</strong>
        </div>
        <div>
          <span>档案中的最大回撤容忍度</span>
          <strong>{snapshot.profile.maxDrawdownPct.toFixed(1)}%</strong>
        </div>
      </div>
      {available &&
        result.lossPct != null &&
        result.lossPct > snapshot.profile.maxDrawdownPct && (
          <p className="dashboard-note loss">
            该假设下的资产降幅超过已填写的回撤容忍度。
          </p>
        )}
      {!available && (
        <p className="dashboard-note">录入资产并补齐汇率后可调整情景。</p>
      )}
      <p className="dashboard-note">
        假设所有非现金资产同比例下跌，外币折算冲击同时作用于外币现金与投资，负债保持不变。仅用于查看承受金额，不含相关性、税费及负债币种变化，不是概率预测；填写的容忍度不等于实际风险容量。
      </p>
    </section>
  );
}
