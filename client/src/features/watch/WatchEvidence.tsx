import type { ReactNode } from "react";
import type { WatchMarketRow } from "../../api/watch";
import { BandTrack, RuleChecks, YieldGrid } from "./WatchSignals";
import { WatchDividends } from "./WatchDividends";
import { number } from "./presentation";

export function WatchEvidence({
  row,
  minYield,
  tolerance,
  children,
}: {
  row: WatchMarketRow;
  minYield: number;
  tolerance: number;
  children?: ReactNode;
}) {
  const { latest } = row;
  return (
    <>
      <div className="watch-metrics">
        <div>
          <span>收盘价 · {latest?.observedOn ?? "—"}</span>
          <strong>{number(latest?.price)}</strong>
        </div>
        <div>
          <span>TTM 税前股息率</span>
          <strong>{number(latest?.yieldPct, "%")}</strong>
        </div>
        <div>
          <span>TTM 每股分红 / 元</span>
          <strong>{latest?.dividendPerShare ?? "—"}</strong>
        </div>
      </div>
      <RuleChecks row={row} minYield={minYield} />
      <div className="watch-bands">
        <BandTrack
          label="日线"
          band={latest?.daily}
          price={latest?.price}
          tolerance={tolerance}
        />
        <BandTrack
          label="周线"
          band={latest?.weekly}
          price={latest?.price}
          tolerance={tolerance}
        />
      </div>
      <YieldGrid row={row} />
      {children}
      <WatchDividends latest={latest} />
    </>
  );
}
