import type { CodexBridgeStatus } from "../../api/settings";

export function CodexModelSelect({
  value,
  onChange,
  bridgeStatus,
}: {
  value: string;
  onChange: (value: string) => void;
  bridgeStatus: CodexBridgeStatus | null;
}) {
  const models = bridgeStatus?.models ?? [];
  const available = !!bridgeStatus?.online && models.length > 0;
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
            请选择模型
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
        {(!bridgeStatus?.online || models.length === 0) && (
          <small>
            {!bridgeStatus?.online
              ? "本机桥接未在线"
              : "请更新并重启本机桥接以读取模型列表"}
          </small>
        )}
      </label>
    </div>
  );
}
