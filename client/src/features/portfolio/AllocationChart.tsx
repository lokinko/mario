import { Segmented } from "antd";
import { useEffect, useState } from "react";
import { Money } from "../../components/AmountPrivacy";
import { getAllocation, type Allocation } from "../../api/portfolio";
import { formatAmount } from "../../lib/format";
import type { Snapshot } from "../../types/user";

const colors = [
  "var(--info)",
  "var(--danger)",
  "var(--success)",
  "var(--warning)",
  "var(--accent)",
  "#9a7fb8",
  "#5f9ea0",
];

function arc(start: number, end: number) {
  const point = (pct: number) => {
    const angle = (2 * Math.PI * pct) / 100 - Math.PI / 2;
    return `${(50 + 40 * Math.cos(angle)).toFixed(3)} ${(50 + 40 * Math.sin(angle)).toFixed(3)}`;
  };
  return `M ${point(start)} A 40 40 0 ${end - start > 50 ? 1 : 0} 1 ${point(end)}`;
}

export function AllocationChart({ snapshot }: { snapshot: Snapshot }) {
  const [mode, setMode] = useState<"byClass" | "byIndustry">("byClass");
  const [allocation, setAllocation] = useState<Allocation | null>(null);
  useEffect(() => {
    let active = true;
    getAllocation()
      .then((next) => active && setAllocation(next))
      .catch(() => active && setAllocation(null));
    return () => {
      active = false;
    };
  }, [snapshot]);
  const slices = allocation?.[mode] ?? [];
  if (!allocation || !slices.length) return null;
  let cursor = 0;
  return (
    <section className="panel allocation-panel" aria-label="资产配置">
      <div className="allocation-header">
        <h2>资产配置</h2>
        <Segmented
          aria-label="资产配置维度"
          size="small"
          value={mode}
          options={[
            { value: "byClass", label: "资产类别" },
            { value: "byIndustry", label: "行业" },
          ]}
          onChange={(value) => setMode(value as typeof mode)}
        />
      </div>
      <div className="allocation-body">
        <svg
          className="allocation-chart"
          viewBox="0 0 100 100"
          role="img"
          aria-label={slices.map((s) => `${s.label} ${s.pct}%`).join("，")}
        >
          {slices.length === 1 ? (
            <circle
              cx="50"
              cy="50"
              r="40"
              fill="none"
              stroke={colors[0]}
              strokeWidth="16"
            />
          ) : (
            slices.map((slice, index) => {
              const start = cursor;
              cursor =
                index === slices.length - 1 ? 100 : start + Number(slice.pct);
              return (
                <path
                  key={slice.label}
                  d={arc(start, cursor)}
                  fill="none"
                  stroke={colors[index % colors.length]}
                  strokeWidth="16"
                />
              );
            })
          )}
        </svg>
        <ul className="allocation-legend">
          {slices.map((slice, index) => (
            <li key={slice.label}>
              <i
                style={{ background: colors[index % colors.length] }}
                aria-hidden="true"
              />
              <span>{slice.label}</span>
              <strong className="num">{slice.pct}%</strong>
              <Money className="num">
                {formatAmount(slice.value, allocation.baseCurrency)}
              </Money>
            </li>
          ))}
        </ul>
      </div>
      {allocation.missing > 0 && (
        <p className="allocation-note">
          {allocation.missing} 项缺少汇率或待处理，未计入。
        </p>
      )}
    </section>
  );
}
