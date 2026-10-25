import { AssetTrend } from "./AssetTrend";
import { useAssetHistory } from "./useAssetHistory";
import { Callout } from "../../components/Callout";
import { Skeleton } from "../../components/Skeleton";
export function AssetHistory({
  state,
}: {
  state: ReturnType<typeof useAssetHistory>;
}) {
  const { history, range, setRange, error, loading, loadMore, retry } = state;
  return (
    <section
      className="panel daily-assets"
      id="asset-history"
      aria-label="资产趋势"
    >
      <div className="panel-title">
        <h2>资产趋势</h2>
        <small title={`记账时区：${history?.timezone ?? ""}`}>
          {history?.records[0]?.day}
          {loading ? " · 读取中…" : ""}
        </small>
      </div>
      {error && (
        <Callout
          tone="danger"
          action={
            <button className="text-button" onClick={retry}>
              重试读取
            </button>
          }
        >
          {error}
        </Callout>
      )}
      <div
        className="section-switcher"
        role="group"
        aria-label="资产趋势范围"
        data-presentation="true"
      >
        {[
          [30, "30 天"],
          [90, "90 天"],
          [365, "1 年"],
          [0, "全部历史"],
        ].map(([n, label]) => (
          <button
            key={n}
            aria-pressed={range === n}
            onClick={() => setRange(Number(n))}
          >
            {label}
          </button>
        ))}
      </div>
      {loading && !history ? (
        <Skeleton rows={3} />
      ) : history?.records.length ? (
        <AssetTrend records={history.records} />
      ) : (
        <p className="empty">
          记录你的第一笔资产后，这里会自动追踪每天的变化。
        </p>
      )}
      {range === 0 && history?.nextBefore && (
        <button
          className="secondary"
          disabled={loading}
          onClick={() => void loadMore()}
        >
          加载更早记录
        </button>
      )}
    </section>
  );
}
