import { getWatchCatalogDetail } from "../../api/watch";
import { useWatchDetail } from "./useWatchDetail";
import type { WatchRules, CatalogRow } from "../../api/watch";
import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { WatchEvidence } from "./WatchEvidence";

export function WatchCatalogDetails({
  row: summary,
  rules,
  busy,
  error,
  joined,
  canAdd,
  onClose,
  onAdd,
  onRetry,
}: {
  row: CatalogRow;
  rules: WatchRules;
  busy: boolean;
  error: string;
  joined: boolean;
  canAdd: boolean;
  onClose: () => void;
  onAdd: () => void;
  onRetry: () => void;
}) {
  const detail = useWatchDetail(
    summary,
    summary.company.symbol,
    getWatchCatalogDetail,
  );
  const row = detail.row;
  const directoryOnly = row.company.symbol.endsWith(".BJ");
  return (
    <Drawer title={row.company.name} onClose={onClose} busy={busy} size="xl">
      <div className="watch-detail-body">
        {error && <Callout tone="danger">{error}</Callout>}
        <WatchEvidence
          row={row}
          rules={row.latest?.rules ?? rules}
          loading={detail.loading}
          detailError={detail.error}
          onReadRetry={detail.retry}
        />
        {directoryOnly ? (
          <p>北交所股票仅收录在公司目录，暂不支持行情观察和加入自选。</p>
        ) : (
          <>
            <div className="watch-actions">
              {!joined && (
                <button
                  className="primary"
                  disabled={busy || !canAdd}
                  onClick={onAdd}
                >
                  加入自选
                </button>
              )}
              <button className="secondary" disabled={busy} onClick={onRetry}>
                重新检查数据
              </button>
            </div>
            {!joined && !canAdd && (
              <p>自选已达 50 家，请先在自选中移除公司后再添加。</p>
            )}
          </>
        )}
      </div>
    </Drawer>
  );
}
