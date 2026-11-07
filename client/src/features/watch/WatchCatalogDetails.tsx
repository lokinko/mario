import type { WatchRules, CatalogRow } from "../../api/watch";
import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { WatchEvidence } from "./WatchEvidence";
import { statuses } from "./presentation";

export function WatchCatalogDetails({
  row,
  industry,
  verifiedOn,
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
  verifiedOn: string;
  rules: WatchRules;
  busy: boolean;
  error: string;
  joined: boolean;
  canAdd: boolean;
  onClose: () => void;
  onAdd: () => void;
  onRetry: () => void;
}) {
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
        <WatchEvidence row={row} rules={rules} />
        <section className="watch-detail-section">
          <h3>公司与行业来源</h3>
          <p className="watch-meta">
            目录更新于 {verifiedOn} · 按主营业务归入{industry}，用于同行比较。
          </p>
          <p className="watch-meta">
            {row.company.sourcesCheckedOn
              ? `公开来源核查于 ${row.company.sourcesCheckedOn}`
              : "公开来源待补充，系统会在后续检查时重试"}
          </p>
          <div className="watch-sources">
            {row.company.sources.map((url, i) => (
              <a key={url} href={url} target="_blank" rel="noreferrer">
                {i === 0 ? "交易所公司资料" : "公司公开报告"} ↗
              </a>
            ))}
          </div>
        </section>
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
        <p className="watch-meta">
          加入自选后可保存关注理由、设置个人网格，并跟踪变化与周内记录。
        </p>
        <p className="watch-meta">
          最近检查{" "}
          {row.checkedAt
            ? new Date(row.checkedAt).toLocaleString("zh-CN", {
                timeZone: "Asia/Shanghai",
              })
            : "—"}
        </p>
      </div>
    </Drawer>
  );
}
