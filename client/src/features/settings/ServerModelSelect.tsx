import { useEffect, useState } from "react";
import { getServerModels, type ServerModelCatalog } from "../../api/settings";

export function ServerModelSelect({
  value,
  onChange,
  defaultModel,
  disabled,
}: {
  value: string;
  onChange: (value: string) => void;
  defaultModel: string;
  disabled: boolean;
}) {
  const [catalog, setCatalog] = useState<ServerModelCatalog | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    setLoading(true);
    setError("");
    getServerModels()
      .then((next) => {
        if (active) setCatalog(next);
      })
      .catch(() => {
        if (active)
          setError("模型目录暂时无法读取，当前选择仍保留。请稍后重新获取。");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [attempt]);
  const preferred = catalog?.defaultModel ?? defaultModel;
  const models = [
    ...new Set([preferred, value, ...(catalog?.models ?? [])].filter(Boolean)),
  ];
  return (
    <div className="form-grid model-fields">
      <label>
        <span>站点模型</span>
        <select
          aria-label="站点模型"
          value={value}
          disabled={disabled}
          onChange={(event) => onChange(event.target.value)}
        >
          {models.map((id) => (
            <option key={id} value={id}>
              {id}
              {id === preferred
                ? " · 站点默认"
                : catalog && !catalog.models.includes(id)
                  ? " · 已保存"
                  : ""}
            </option>
          ))}
        </select>
        <small>
          {loading
            ? "正在获取站点可用模型…"
            : error ||
              "目录来自站点 OpenAI 项目；列出的文本候选仍需通过连接测试确认可调用。"}
        </small>
      </label>
      <div>
        <button
          type="button"
          className="secondary"
          disabled={disabled || loading}
          onClick={() => setAttempt((n) => n + 1)}
        >
          重新获取模型
        </button>
        <p>无需填写个人 API Key，调用费用由站点承担。选择后点击“保存配置”。</p>
      </div>
    </div>
  );
}
