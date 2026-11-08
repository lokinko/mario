import { getWatchCatalogDetail } from "../../api/watch";
import { useWatchDetail } from "./useWatchDetail";
import type { WatchRules, CatalogRow } from "../../api/watch";
import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { WatchEvidence } from "./WatchEvidence";
import { statuses } from "./presentation";

export function WatchCatalogDetails({
  row: summary,
  industry,
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
  industry: string;
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
  return (
    <Drawer
      title={row.company.name}
      subtitle={`${row.company.symbol} · ${industry} · ${row.latest ? statuses[row.latest.status] : "等待首次检查"}`}
      onClose={onClose}
      busy={busy}
      size="wide"
    >
      <div className="watch-detail-body">
        {error && <Callout tone="danger">{error}</Callout>}
        <WatchEvidence
          row={row}
          rules={row.latest?.rules ?? rules}
          companySources={row.company.sources}
          loading={detail.loading}
          detailError={detail.error}
          onReadRetry={detail.retry}
        />
        <div className="watch-actions">
          <button
            className="primary"
            disabled={busy || joined || !canAdd}
            onClick={onAdd}
          >
            {joined ? "已在自选" : "加入自选"}
          </button>
          <button className="secondary" disabled={busy} onClick={onRetry}>
            重新检查数据
          </button>
        </div>
        {!joined && !canAdd && (
          <p>自选已达 50 家，请先在自选中移除公司后再添加。</p>
        )}
      </div>
    </Drawer>
  );
}
