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
        <small>
          {!bridgeStatus?.online
            ? "本机桥接在线后会显示模型列表，已保存的选择会保留。"
            : models.length === 0
              ? "请更新并重启本机桥接以读取模型列表。"
              : "选择本机 Codex 提供的模型，点击“保存配置”后用于后续问答。模型范围取决于 Codex 和账号权限。"}
        </small>
      </label>
    </div>
  );
}
