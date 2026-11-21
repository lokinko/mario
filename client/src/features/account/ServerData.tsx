import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { useState } from "react";
import { Download, LoaderCircle, Upload } from "lucide-react";
import {
  exportData,
  importData,
  prepareDataReplacement,
} from "../../api/account";
import type { SyncState } from "../../lib/autoSync";
import { saveTextDownload } from "../../lib/download";
import { localDateValue } from "../../lib/dates";
import { useRequestGuard } from "../../lib/useRequestGuard";
import { serviceUrlStorageKey } from "../../lib/service";
import { AccountSettings } from "./WebAccess";
import { ServerAddress } from "./ServerAddress";

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
  const [selectedAccount, setSelectedAccount] = useState("");
  const [backupConfirmed, setBackupConfirmed] = useState(false);
  const [hasCustomServer] = useState(() =>
    Boolean(localStorage.getItem(serviceUrlStorageKey())),
  );
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
    } catch (nextError) {
      if (isCurrent()) setError(String(nextError));
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
    } catch (nextError) {
      if (isCurrent()) setError(String(nextError));
    } finally {
      if (isCurrent()) setOperation(null);
      input.value = "";
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
  const syncAttention =
    autoState.phase === "offline" ||
    autoState.phase === "attention" ||
    autoState.phase === "paused";
  return (
    <div className="account-data" id="account-settings">
      <AccountSettings />
      {syncAttention && (
        <Callout
          tone="warning"
          action={
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={onRetryAuto}
            >
              重试同步
            </button>
          }
        >
          {autoState.message}
        </Callout>
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
              data-presentation="true"
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
            </div>
          </div>
        </div>
      </section>
      <details
        className="panel server-address-details"
        open={hasCustomServer || undefined}
      >
        <summary>高级：自定义服务器地址</summary>
        <div className="server-address-details-body">
          <ServerAddress />
        </div>
      </details>
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
              ? "将替换当前账号的全部业务数据（不做合并），覆盖前会自动下载当前备份；失败会整体回滚。"
              : "将此文件导入当前空账号，失败会整体回滚。"}
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
                  将导入 {selectedSource.stocks} 只自选证券、
                  {selectedSource.transactions} 条交易记录（成本按交易重算）。
                </p>
              )}
            </>
          )}
          {pending.replace && (
            <label className="data-replacement-option" data-presentation="true">
              <input
                type="checkbox"
                checked={backupConfirmed}
                disabled={busy}
                onChange={(event) => setBackupConfirmed(event.target.checked)}
              />
              我确认覆盖当前账号的全部业务数据（覆盖前将自动下载备份）
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
    </div>
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
  if (!/^[12](\.|$)/.test(String(root.version)) || !root.data)
    throw new Error("仅支持 version 为 1.x 或 2.x 的自选 JSON");
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
