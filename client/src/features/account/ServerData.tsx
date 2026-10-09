import { useState } from "react";
import { Cloud, Download, LoaderCircle, RefreshCw, Upload } from "lucide-react";
import { exportData, importData } from "../../api";
import { PageHeader } from "../../components/PageHeader";
import type { SyncState } from "../../lib/autoSync";
import { serviceUrl } from "../../lib/service";
import { localDateValue } from "../../lib/dates";
export function ServerData({
  flash,
  onRestore,
  autoState,
  onRetryAuto,
}: {
  flash: (s: string) => void;
  onRestore: () => void | Promise<void>;
  autoState: SyncState;
  onRetryAuto: () => void;
}) {
  const [operation, setOperation] = useState<"export" | "import" | null>(null);
  const busy = operation !== null;
  const [error, setError] = useState("");
  return (
    <section className="page narrow cloud-page">
      <PageHeader title="账号与数据" />
      <section className="panel server-connection">
        <div className="panel-title">
          <h2>
            <Cloud size={18} />
            服务器连接
          </h2>
          <button
            className="secondary"
            onClick={onRetryAuto}
            disabled={busy || autoState.phase === "syncing"}
          >
            <RefreshCw
              size={15}
              className={autoState.phase === "syncing" ? "spin" : undefined}
            />
            {autoState.phase === "syncing" ? "检查中…" : "检查最新数据"}
          </button>
        </div>
        <div className="server-address">
          <span>当前服务器</span>
          <code>{serviceUrl()}</code>
        </div>
        <p className={`sync-live-status sync-${autoState.phase}`} role="status">
          {autoState.message}
        </p>
        <details className="inline-help">
          <summary>同步与离线说明</summary>
          <p>
            同一账号跨设备同步；未保存的输入会先保留并提示核对。离线时无法保存。
          </p>
        </details>
      </section>
      <div className="data-transfer-grid">
        <section className="panel data-transfer-card">
          <div className="panel-title">
            <h2>
              <Download size={18} />
              导出备份
            </h2>
          </div>
          <p className="section-intro">
            含个人资料，请妥善保管。密钥和账号会话不导出。
          </p>
          <button
            className="secondary"
            disabled={busy}
            onClick={async () => {
              setOperation("export");
              setError("");
              try {
                const data = await exportData();
                const url = URL.createObjectURL(
                  new Blob([JSON.stringify(data)], {
                    type: "application/json",
                  }),
                );
                const link = document.createElement("a");
                link.href = url;
                link.download = `mario-data-${localDateValue(new Date())}.json`;
                link.click();
                setTimeout(() => URL.revokeObjectURL(url), 1000);
              } catch (e) {
                setError(String(e));
              } finally {
                setOperation(null);
              }
            }}
          >
            {operation === "export" ? (
              <LoaderCircle size={16} className="spin" />
            ) : (
              <Download size={16} />
            )}
            {operation === "export" ? "导出中…" : "导出我的数据"}
          </button>
        </section>
        <section className="panel data-transfer-card">
          <div className="panel-title">
            <h2>
              <Upload size={18} />
              导入数据
            </h2>
          </div>
          <p className="section-intro">
            仅适用于空账号，支持 JSON 文件，最大 8 MB。
          </p>
          <label
            className={`secondary file-import-button ${busy ? "disabled" : ""}`}
          >
            {operation === "import" ? (
              <LoaderCircle size={16} className="spin" />
            ) : (
              <Upload size={16} />
            )}
            {operation === "import" ? "导入中…" : "导入到空账号"}
            <input
              type="file"
              accept="application/json,.json"
              disabled={busy}
              onChange={async (e) => {
                const input = e.currentTarget;
                const file = input.files?.[0];
                if (!file) return;
                setOperation("import");
                setError("");
                try {
                  if (file.size > 8 * 1024 * 1024)
                    throw new Error("文件超过 8 MB");
                  const data = JSON.parse(await file.text());
                  if (!confirm("将把此文件导入当前空账号，是否继续？")) return;
                  await importData(data);
                  await onRestore();
                  flash("数据已导入");
                } catch (error) {
                  setError(String(error));
                } finally {
                  setOperation(null);
                  input.value = "";
                }
              }}
            />
          </label>
        </section>
      </div>
      {error && (
        <p className="error-box" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
