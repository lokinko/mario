import { useState } from "react";
import { Callout } from "../../components/Callout";
import { probeService } from "../../api/account";
import {
  defaultServiceUrl,
  hasCustomServiceUrl,
  normalizeServiceUrl,
  serviceUrl,
  setServiceUrl,
} from "../../lib/service";

/** Not a form: it also renders inside the login form. */
export function ServerAddress({
  onSwitched = () => window.location.reload(),
}: {
  onSwitched?: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(serviceUrl);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const apply = async (next: string | null) => {
    if (busy) return;
    setError("");
    let target: string;
    try {
      target = normalizeServiceUrl(next ?? defaultServiceUrl());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return;
    }
    if (!window.confirm("切换服务器会重新加载页面，未保存的输入将丢失。"))
      return;
    setBusy(true);
    try {
      await probeService(target);
      setServiceUrl(next === null ? null : target);
      onSwitched();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  };
  const cancel = () => {
    setEditing(false);
    setValue(serviceUrl());
    setError("");
  };
  return (
    <div className="server-address">
      <span>服务器地址</span>
      {editing ? (
        <div className="server-address-edit">
          <input
            aria-label="服务器地址"
            value={value}
            disabled={busy}
            autoFocus
            placeholder="https://example.com/api"
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void apply(value);
              } else if (e.key === "Escape") cancel();
            }}
          />
          <button
            type="button"
            className="primary"
            disabled={busy || !value.trim()}
            onClick={() => void apply(value)}
          >
            {busy ? "连接中…" : "保存并重新连接"}
          </button>
          <button
            type="button"
            className="text-button"
            disabled={busy}
            onClick={cancel}
          >
            取消
          </button>
        </div>
      ) : (
        <div className="server-address-current">
          <code>{serviceUrl()}</code>
          <button
            type="button"
            className="text-button"
            onClick={() => setEditing(true)}
          >
            修改
          </button>
          {hasCustomServiceUrl() && (
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={() => void apply(null)}
            >
              恢复默认
            </button>
          )}
        </div>
      )}
      {error && <Callout tone="danger">{error}</Callout>}
    </div>
  );
}
