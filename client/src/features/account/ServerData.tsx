import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { useState } from "react";
import {
  ClipboardPaste,
  Cloud,
  Download,
  LoaderCircle,
  RefreshCw,
  Upload,
} from "lucide-react";
import {
  exportData,
  importData,
  prepareDataReplacement,
} from "../../api/account";
import { PageHeader } from "../../components/PageHeader";
import type { SyncState } from "../../lib/autoSync";
import { saveTextDownload } from "../../lib/download";
import { localDateValue } from "../../lib/dates";
import { useRequestGuard } from "../../lib/useRequestGuard";
import { AccountSettings } from "./WebAccess";
import { ServerAddress } from "./ServerAddress";
export function ServerData({
  flash,
  onRestore,
  autoState,
  onRetryAuto,
  transferOnly = false,
}: {
  flash: (s: string) => void;
  onRestore: () => void | Promise<void>;
  autoState: SyncState;
  onRetryAuto: () => void;
  transferOnly?: boolean;
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
  const [jsonText, setJsonText] = useState("");
  const [pasting, setPasting] = useState(false);
  const [selectedAccount, setSelectedAccount] = useState("");
  const [backupConfirmed, setBackupConfirmed] = useState(false);
  const source = pending ? describeImport(pending.data) : null;
  const selectedSource = source?.accounts.find(
    (account) => account.id === selectedAccount,
  );
  const ready = !source || !!selectedSource;
  const stageImport = (data: unknown) => {
    const description = describeImport(data);
    setSelectedAccount(
      description?.accounts.length === 1 ? description.accounts[0].id : "",
    );
    setPending({ data, replace });
    setBackupConfirmed(false);
  };
  const confirmImport = async () => {
    if (!pending || busy || !ready || (pending.replace && !backupConfirmed))
      return;
    const data = source?.hasAccounts
      ? { ...(pending.data as object), selectedAccountId: selectedAccount }
      : pending.data;
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
        await backup.replace(data);
      } else {
        await importData(data);
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
  const chooseFile = async (input: HTMLInputElement) => {
    const file = input.files?.[0];
    if (!file) return;
    const isCurrent = requests.begin();
    setOperation("import");
    setError("");
    try {
      if (file.size > 64 * 1024 * 1024) throw new Error("文件超过 64 MB");
      const data = JSON.parse(await file.text());
      if (!isCurrent()) return;
      stageImport(data);
    } catch (error) {
      if (isCurrent()) setError(String(error));
    } finally {
      if (isCurrent()) setOperation(null);
      input.value = "";
    }
  };
  const previewPasted = () => {
    setError("");
    try {
      if (new Blob([jsonText]).size > 64 * 1024 * 1024)
        throw new Error("数据超过 64 MB");
      stageImport(JSON.parse(jsonText));
    } catch (error) {
      setError(String(error));
    }
  };
  const exportBackup = async () => {
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
  };
  return (
    <section
      className={transferOnly ? undefined : "page narrow cloud-page"}
      id={transferOnly ? "data-settings" : undefined}
    >
      {!transferOnly && (
        <>
          <PageHeader title="账号与数据" />
          <AccountSettings />
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
            <ServerAddress />
            <p
              className={`sync-live-status sync-${autoState.phase}`}
              role="status"
            >
              {autoState.message}
            </p>
            <details className="inline-help">
              <summary>同步与离线说明</summary>
              <p>
                同一账号跨设备同步；未保存的输入会先保留并提示核对。离线时无法保存。
              </p>
            </details>
          </section>
        </>
      )}
      <section className="panel data-transfer" aria-label="备份与导入">
        <div className="data-transfer-row">
          <h3>
            <Download size={16} />
            导出备份
          </h3>
          <button
            className="secondary"
            disabled={busy}
            onClick={() => void exportBackup()}
          >
            {operation === "export" ? (
              <LoaderCircle size={16} className="spin" />
            ) : (
              <Download size={16} />
            )}
            {operation === "export" ? "导出中…" : "导出我的数据"}
          </button>
        </div>
        <div className="data-transfer-row data-import">
          <h3>
            <Upload size={16} />
            导入数据
          </h3>
          <div className="data-import-body">
            <div
              className="data-import-mode"
              role="radiogroup"
              aria-label="导入方式"
            >
              {[
                { replace: false, label: "导入到空账号" },
                { replace: true, label: "覆盖当前账号" },
              ].map((mode) => (
                <label
                  key={mode.label}
                  className={replace === mode.replace ? "selected" : ""}
                >
                  <input
                    type="radio"
                    name="data-import-mode"
                    checked={replace === mode.replace}
                    disabled={busy}
                    onChange={() => setReplace(mode.replace)}
                  />
                  {mode.label}
                </label>
              ))}
            </div>
            {replace && (
              <Callout tone="warning">
                将替换当前账号的全部业务数据，不做合并；覆盖前会自动下载当前数据的备份。
              </Callout>
            )}
            <div className="data-import-actions">
              <label
                className={`secondary file-import-button ${busy ? "disabled" : ""}`}
              >
                {operation === "import" ? (
                  <LoaderCircle size={16} className="spin" />
                ) : (
                  <Upload size={16} />
                )}
                {operation === "import" ? "导入中…" : "选择 JSON 文件"}
                <input
                  type="file"
                  accept="application/json,.json"
                  disabled={busy}
                  onChange={(e) => void chooseFile(e.currentTarget)}
                />
              </label>
              <button
                type="button"
                className="secondary"
                aria-expanded={pasting}
                disabled={busy}
                onClick={() => setPasting(!pasting)}
              >
                <ClipboardPaste size={16} />
                粘贴 JSON
              </button>
            </div>
            {pasting && (
              <div className="data-import-paste">
                <textarea
                  aria-label="粘贴 JSON 数据"
                  rows={6}
                  value={jsonText}
                  disabled={busy}
                  autoFocus
                  onChange={(event) => setJsonText(event.target.value)}
                  placeholder={'{"version":"1.0","exportedAt":"…","data":{…}}'}
                />
                <button
                  type="button"
                  className="primary"
                  disabled={busy || !jsonText.trim()}
                  onClick={previewPasted}
                >
                  预览导入
                </button>
              </div>
            )}
          </div>
        </div>
      </section>
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
          {source && (
            <>
              <label data-presentation="true">
                <span>来源账户</span>
                <select
                  aria-label="来源账户"
                  value={selectedAccount}
                  disabled={busy}
                  onChange={(event) => setSelectedAccount(event.target.value)}
                >
                  <option value="" disabled>
                    请选择账户
                  </option>
                  {source.accounts.map((account) => (
                    <option key={account.id} value={account.id}>
                      {account.name}
                    </option>
                  ))}
                </select>
              </label>
              {selectedSource && (
                <p role="status">
                  将导入 {selectedSource.stocks} 只自选股、
                  {selectedSource.transactions}{" "}
                  条源交易记录（红利税合入对应卖出的税费）。
                </p>
              )}
              <p>
                买卖按上海日期和原时间顺序录入；分红金额按每股金额 ×
                股数计算。成本按完整交易重算，分红单列收益，可能与源 costPrice
                不同。原 price 仅作未核验参考价。
              </p>
              <p>
                只读取所选账户的自选和账本，不重复导入顶层
                watchlist。现金追踪未启用时不导入占位余额；已启用时保留当前余额、待核对，历史成交不再次扣现金。原手续费估算启用或红利税无法匹配时拒绝整次导入。
              </p>
              <p>
                行业、目标价、旧股息率、行情缓存和发现列表不导入；保留当前账号观察规则。请保留原
                JSON 以核对来源。
              </p>
            </>
          )}
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
              disabled={busy || !ready || (pending.replace && !backupConfirmed)}
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

function describeImport(data: unknown): {
  hasAccounts: boolean;
  accounts: {
    id: string;
    name: string;
    stocks: number;
    transactions: number;
  }[];
} | null {
  if (!data || typeof data !== "object" || !("version" in data)) return null;
  const root = data as {
    version?: unknown;
    data?: { accounts?: unknown; watchlist?: unknown };
  };
  if (root.version !== "1.0" || !root.data)
    throw new Error("仅支持 version 为 1.0 的自选 JSON");
  const hasAccounts = root.data.accounts !== undefined;
  const accounts = hasAccounts
    ? root.data.accounts
    : [{ id: "default", name: "导入账户", watchlist: root.data.watchlist }];
  if (!Array.isArray(accounts) || !accounts.length)
    throw new Error("JSON 缺少账户或自选列表");
  const ids = new Set<string>();
  return {
    hasAccounts,
    accounts: accounts.map((account) => {
      if (
        !account ||
        typeof account !== "object" ||
        typeof account.id !== "string" ||
        !account.id.trim() ||
        ids.has(account.id) ||
        typeof account.name !== "string" ||
        !Array.isArray(account.watchlist)
      )
        throw new Error("来源账户格式无效或编号重复");
      ids.add(account.id);
      return {
        id: account.id,
        name: account.name,
        stocks: account.watchlist.length,
        transactions: account.watchlist.reduce(
          (sum: number, stock: { transactions?: unknown }) =>
            sum +
            (Array.isArray(stock?.transactions)
              ? stock.transactions.length
              : 0),
          0,
        ),
      };
    }),
  };
}
