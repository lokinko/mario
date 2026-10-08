import { useState } from "react";
import { exportData, importData } from "../../api";
import type { SyncState } from "../../lib/autoSync";
import { serviceUrl } from "../../lib/service";
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
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  return (
    <section className="page narrow">
      <h1>账号与数据</h1>
      <div className="panel">
        <p>当前服务器：{serviceUrl()}</p>
        <p>{autoState.message}</p>
        <p>
          网页和客户端直接读写同一账号的数据。其他设备有更新时会刷新页面；有未提交输入时会先提示你核对。离线期间无法保存。
        </p>
        <button onClick={onRetryAuto}>检查最新数据</button>
        <h2>迁移与备份</h2>
        <p>
          导出文件含个人资料，请妥善保管。模型密钥与账号会话不包含在导出文件中。导入仅适用于空账号。
        </p>
        <button
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            setError("");
            try {
              const data = await exportData();
              const url = URL.createObjectURL(
                new Blob([JSON.stringify(data)], { type: "application/json" }),
              );
              const link = document.createElement("a");
              link.href = url;
              link.download = `mario-data-${new Date().toISOString().slice(0, 10)}.json`;
              link.click();
              setTimeout(() => URL.revokeObjectURL(url), 1000);
            } catch (e) {
              setError(String(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          导出我的数据
        </button>
        <label>
          导入到空账号
          <input
            type="file"
            accept="application/json,.json"
            disabled={busy}
            onChange={async (e) => {
              const file = e.target.files?.[0];
              if (!file) return;
              setBusy(true);
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
                setBusy(false);
                e.target.value = "";
              }
            }}
          />
        </label>
        {error && (
          <p className="error-box" role="alert">
            {error}
          </p>
        )}
      </div>
    </section>
  );
}
