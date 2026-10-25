import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { useState } from "react";
import { Cloud, Download, LoaderCircle, RefreshCw, Upload } from "lucide-react";
import {
  exportData,
  importData,
  prepareDataReplacement,
} from "../../api/account";
import { PageHeader } from "../../components/PageHeader";
import type { SyncState } from "../../lib/autoSync";
import { serviceUrl } from "../../lib/service";
import { saveTextDownload } from "../../lib/download";
import { localDateValue } from "../../lib/dates";
import { useRequestGuard } from "../../lib/useRequestGuard";
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
  const [replace, setReplace] = useState(false);
  const requests = useRequestGuard();
  const [pending, setPending] = useState<{
    data: unknown;
    replace: boolean;
  } | null>(null);
  const [backupConfirmed, setBackupConfirmed] = useState(false);
  const confirmImport = async () => {
    if (!pending || busy || (pending.replace && !backupConfirmed)) return;
    const isCurrent = requests.begin();
    setOperation("import");
    setError("");
    try {
      if (pending.replace) {
        const backup = await prepareDataReplacement();
        if (!isCurrent()) return;
        await saveTextDownload(
          `mario-before-import-${new Date().toISOString().replace(/[:.]/g, "-")}.json`,
          JSON.stringify(backup.data),
          "application/json",
        );
        if (!isCurrent()) return;
        await backup.replace(pending.data);
      } else {
        await importData(pending.data);
      }
      if (!isCurrent()) return;
      await onRestore();
      if (isCurrent()) {
        setPending(null);
        flash(
          pending.replace ? "数据已覆盖导入，覆盖前备份已下载" : "数据已导入",
        );
      }
    } catch (error) {
      if (isCurrent()) setError(String(error));
    } finally {
      if (isCurrent()) setOperation(null);
    }
  };
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
                await saveTextDownload(
                  `mario-data-${localDateValue(new Date())}.json`,
                  JSON.stringify(data),
                  "application/json",
                );
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
            支持 JSON 文件，最大 64 MB。默认仅导入空账号。
          </p>
          <label className="data-replacement-option">
            <input
              type="checkbox"
              checked={replace}
              disabled={busy}
              onChange={(e) => setReplace(e.target.checked)}
            />
            覆盖当前账号已有数据
          </label>
          {replace && (
            <p className="section-intro">
              将替换当前账号的全部业务数据，包括交易、持仓和历史记录，不做合并。
              覆盖前会自动下载当前数据的备份，请确认浏览器允许下载并妥善保管。
            </p>
          )}
          <label
            className={`secondary file-import-button ${busy ? "disabled" : ""}`}
          >
            {operation === "import" ? (
              <LoaderCircle size={16} className="spin" />
            ) : (
              <Upload size={16} />
            )}
            {operation === "import"
              ? "导入中…"
              : replace
                ? "选择文件并覆盖导入"
                : "导入到空账号"}
            <input
              type="file"
              accept="application/json,.json"
              disabled={busy}
              onChange={async (e) => {
                const input = e.currentTarget;
                const file = input.files?.[0];
                if (!file) return;
                const isCurrent = requests.begin();
                setOperation("import");
                setError("");
                try {
                  if (file.size > 64 * 1024 * 1024)
                    throw new Error("文件超过 64 MB");
                  const data = JSON.parse(await file.text());
                  if (!isCurrent()) return;
                  setPending({ data, replace });
                  setBackupConfirmed(false);
                } catch (error) {
                  if (isCurrent()) setError(String(error));
                } finally {
                  if (isCurrent()) setOperation(null);
                  input.value = "";
                }
              }}
            />
          </label>
        </section>
      </div>
      {pending && (
        <Drawer
          title={pending.replace ? "确认覆盖当前账号数据" : "确认导入数据"}
          busy={busy}
          onClose={() => {
            setPending(null);
            setError("");
          }}
        >
          <Callout tone={pending.replace ? "warning" : "info"}>
            {pending.replace
              ? "将替换当前账号的全部业务数据，包括持仓、成交、现金、方法论、参考资料和历史记录，不做合并。账号登录凭据不变。"
              : "将此文件导入当前空账号。"}
          </Callout>
          <p>
            导入失败会整体回滚；覆盖前会额外下载当前数据的备份。如需恢复，须用备份文件重新导入。
          </p>
          {pending.replace && (
            <label className="data-replacement-option" data-presentation="true">
              <input
                type="checkbox"
                checked={backupConfirmed}
                disabled={busy}
                onChange={(event) => setBackupConfirmed(event.target.checked)}
              />
              我已导出当前数据
            </label>
          )}
          {error && <Callout tone="danger">{error}</Callout>}
          <div className="form-actions">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => setPending(null)}
            >
              取消
            </button>
            <button
              className={`primary${pending.replace ? " danger" : ""}`}
              disabled={busy || (pending.replace && !backupConfirmed)}
              onClick={() => void confirmImport()}
            >
              {busy ? "导入中…" : pending.replace ? "确认覆盖导入" : "确认导入"}
            </button>
          </div>
        </Drawer>
      )}
      {error && !pending && <Callout tone="danger">{error}</Callout>}
    </section>
  );
}
