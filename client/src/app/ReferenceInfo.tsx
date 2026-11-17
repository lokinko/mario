import { lazy, useEffect, useState } from "react";
import { getWatchBoard } from "../api/watch";
import { DeferredPage } from "../components/DeferredPage";
import { PageHeader } from "../components/PageHeader";
import type { Snapshot } from "../types";

const EvidenceWorkbench = lazy(() =>
  import("../features/evidence/EvidenceWorkbench").then((m) => ({
    default: m.EvidenceWorkbench,
  })),
);
const ReviewCenter = lazy(() =>
  import("../features/reviews/ReviewCenter").then((m) => ({
    default: m.ReviewCenter,
  })),
);

export type ReferenceTab = "evidence" | "reviews" | "rules";

const tabs: { id: ReferenceTab; label: string }[] = [
  { id: "evidence", label: "参考资料" },
  { id: "reviews", label: "复盘记录" },
  { id: "rules", label: "投资规则" },
];

export function ReferenceInfo({
  tab,
  onTabChange,
  snapshot,
  onAsk,
  flash,
}: {
  tab: ReferenceTab;
  onTabChange: (tab: ReferenceTab) => void;
  snapshot: Snapshot;
  onAsk: (question: string) => void;
  flash: (s: string) => void;
}) {
  const [watched, setWatched] = useState<{ name: string; symbol: string }[]>(
    [],
  );
  useEffect(() => {
    let current = true;
    getWatchBoard()
      .then((board) => {
        if (current)
          setWatched(
            board.rows.map(({ entry }) => ({
              name: entry.name,
              symbol: entry.symbol,
            })),
          );
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, []);
  const seen = new Set<string>();
  const stocks = [
    ...snapshot.holdings
      .filter((holding) => holding.assetClass === "股票" && holding.symbol)
      .map((holding) => ({ name: holding.name, symbol: holding.symbol })),
    ...watched,
  ].filter((stock) => !seen.has(stock.symbol) && seen.add(stock.symbol));

  return (
    <div className="page narrow">
      <PageHeader title="参考信息" />
      <div className="section-switcher" role="group" aria-label="参考信息分类">
        {tabs.map((item) => (
          <button
            key={item.id}
            aria-pressed={tab === item.id}
            onClick={() => onTabChange(item.id)}
          >
            {item.label}
          </button>
        ))}
      </div>
      <DeferredPage>
        {tab === "evidence" ? (
          <EvidenceWorkbench stocks={stocks} onAsk={onAsk} flash={flash} />
        ) : (
          <ReviewCenter tab={tab} onAsk={onAsk} flash={flash} />
        )}
      </DeferredPage>
    </div>
  );
}
