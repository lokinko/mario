import { useEffect, useState } from "react";
import {
  getCodexModels,
  type CodexModel,
  type CodexBridgeStatus,
} from "../../api/settings";

export function CodexModelSelect({
  provider,
  value,
  onChange,
  bridgeStatus,
}: {
  provider: "codex" | "codex-bridge";
  value: string;
  onChange: (value: string) => void;
  bridgeStatus: CodexBridgeStatus | null;
}) {
  const [localModels, setLocalModels] = useState<CodexModel[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    if (provider !== "codex") return;
    let active = true;
    setLoading(true);
    setError("");
    void getCodexModels()
      .then((models) => {
        if (active) setLocalModels(models);
      })
      .catch((error) => {
        if (active) {
          setLocalModels([]);
          setError(String(error));
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [provider, revision]);
  const models =
    provider === "codex" ? localModels : (bridgeStatus?.models ?? []);
  const available =
    provider === "codex"
      ? !loading && models.length > 0
      : !!bridgeStatus?.online && models.length > 0;
  const selected = models.find((model) => model.model === value);
  return (
    <div className="codex-model-picker">
      <label>
        <span>模型名称</span>
        <select
          aria-label="模型名称"
          value={value}
          disabled={!available}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="" disabled>
            {loading ? "正在读取模型…" : "请选择模型"}
          </option>
          {value && !selected && (
            <option value={value} disabled>
              {value}（已保存，待确认可用）
            </option>
          )}
          {models.map((model) => (
            <option key={model.model} value={model.model}>
              {model.displayName}
              {model.isDefault ? " · 默认" : ""}
            </option>
          ))}
        </select>
        {selected?.description && <small>{selected.description}</small>}
        <small>
          {provider === "codex-bridge" && !bridgeStatus?.online
            ? "本机桥接在线后会显示模型列表，已保存的选择会保留。"
            : provider === "codex-bridge" && models.length === 0
              ? "请更新并重启本机桥接以读取模型列表。"
              : "选择本机 Codex 提供的模型，点击“保存配置”后用于后续问答。模型范围取决于 Codex 和账号权限。"}
        </small>
      </label>
      {provider === "codex" && (
        <button
          type="button"
          className="secondary"
          disabled={loading}
          onClick={() => setRevision((value) => value + 1)}
        >
          重新读取模型
        </button>
      )}
      {error && <small role="alert">读取模型失败：{error}</small>}
    </div>
  );
}
