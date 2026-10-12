import type { Holding } from "../../types";
import { useState } from "react";
import { formatNumber } from "../../lib/format";

export function hasHoldingDetails(holding: Omit<Holding, "id">) {
  return [holding.costPrice, holding.quantity, holding.currentPrice].some(
    (value) => value != null,
  );
}

export function validHoldingDetails(holding: Omit<Holding, "id">) {
  if (!hasHoldingDetails(holding)) return true;
  return (
    holding.quantity != null &&
    (holding.currentPrice != null ||
      (holding.symbol.trim() !== "" && holding.quantity > 0)) &&
    [holding.costPrice, holding.quantity, holding.currentPrice].every(
      (value) =>
        value == null ||
        (Number.isFinite(value) && value >= 0 && value <= 1e15),
    )
  );
}

export function withHoldingTotals<T extends Omit<Holding, "id">>(
  holding: T,
): T {
  if (holding.quantity == null || holding.currentPrice == null) return holding;
  return {
    ...holding,
    marketValue: holding.quantity * holding.currentPrice,
    costBasis:
      holding.costPrice == null
        ? holding.costBasis
        : holding.quantity * holding.costPrice,
  };
}

export function HoldingFields<T extends Omit<Holding, "id">>({
  holding,
  onChange,
  tradeManaged = false,
}: {
  holding: T;
  onChange: (holding: T) => void;
  tradeManaged?: boolean;
}) {
  const [focused, setFocused] = useState<string | null>(null);
  return (
    <>
      {(
        [
          ["costPrice", "成本价（选填）", holding.currency],
          ["quantity", "持仓数", "份 / 股 / 单位"],
          ["currentPrice", "当前价格", holding.currency],
        ] as const
      ).map(([key, label, unit]) => (
        <label key={key}>
          <span>{label}</span>
          <div className="input-affix">
            <input
              type="number"
              inputMode="decimal"
              min={0}
              max={1e15}
              step="any"
              readOnly={tradeManaged && key !== "currentPrice"}
              value={
                holding[key] == null
                  ? ""
                  : focused === key && !(tradeManaged && key !== "currentPrice")
                    ? holding[key]
                    : formatNumber(holding[key])
              }
              onFocus={() => setFocused(key)}
              onBlur={() => setFocused(null)}
              placeholder="手动输入"
              onChange={(event) =>
                onChange({
                  ...withHoldingTotals({
                    ...holding,
                    [key]:
                      event.target.value === ""
                        ? null
                        : Number(event.target.value),
                  }),
                  ...(tradeManaged ? { costBasis: holding.costBasis } : {}),
                })
              }
            />
            <i>{unit}</i>
          </div>
        </label>
      ))}
    </>
  );
}
