import type { ReactNode } from "react";
import type { WatchRules, WatchMarketRow } from "../../api/watch";
import { BandTrack, RuleChecks, YieldGrid } from "./WatchSignals";
import { WatchDividends } from "./WatchDividends";
import { WatchSources } from "./WatchSources";
import { WatchTrends } from "./WatchTrends";
import { number } from "./presentation";

export function WatchEvidence({
  row,
  rules,
  companySources,
  children,
}: {
  row: WatchMarketRow;
  rules: WatchRules;
  companySources?: string[];
  children?: ReactNode;
}) {
  const minYield = rules.minYieldBps / 100;
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
      <RuleChecks row={row} rules={rules} />
      <div className="watch-bands">
        <BandTrack
          label="日线"
          band={latest?.daily}
          price={latest?.price}
          rule={rules.daily}
        />
        <BandTrack
          label="周线"
          band={latest?.weekly}
          price={latest?.price}
          rule={rules.weekly}
        />
      </div>
      <WatchTrends key={latest?.symbol} latest={latest} />
      <YieldGrid row={row} minYield={minYield} />
      {children}
      <WatchDividends latest={latest} />
      <WatchSources company={companySources} observation={latest?.sources} />
    </>
  );
}
