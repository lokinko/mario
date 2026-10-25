import type { Holding } from "../../types";

export function hasHoldingDetails(holding: Omit<Holding, "id">) {
  return [holding.costPrice, holding.quantity, holding.currentPrice].some(
    (value) => value != null,
  );
}

export function holdingExactValues(holding: Omit<Holding, "id">) {
  return (
    holding.exactValues ?? {
      marketValue: String(holding.marketValue),
      costBasis: String(holding.costBasis),
      quantity: holding.quantity == null ? null : String(holding.quantity),
      costPrice: holding.costPrice == null ? null : String(holding.costPrice),
      currentPrice:
        holding.currentPrice == null ? null : String(holding.currentPrice),
    }
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
  return {
    ...holding,
    marketValue:
      holding.quantity == null || holding.currentPrice == null
        ? holding.marketValue
        : holding.quantity * holding.currentPrice,
    costBasis:
      holding.quantity == null || holding.costPrice == null
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
              data-money-input={key !== "quantity" || undefined}
              type="number"
              inputMode="decimal"
              min={0}
              max={1e15}
              step="any"
              readOnly={tradeManaged && key !== "currentPrice"}
              value={holding.exactValues?.[key] ?? holding[key] ?? ""}
              placeholder={
                key === "currentPrice" && holding.symbol.trim()
                  ? "留空自动获取日收盘价"
                  : "手动输入"
              }
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
                  exactValues: {
                    ...holdingExactValues(holding),
                    [key]: event.target.value || null,
                  },
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
