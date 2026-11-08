import { Badge } from "../../components/Badge";
import { Callout } from "../../components/Callout";
import { useState } from "react";
import {
  Bot,
  KeyRound,
  Eye,
  EyeOff,
  LoaderCircle,
  Save,
  Trash2,
} from "lucide-react";
import {
  deleteModelKey,
  saveModelConfig,
  testModelConnection,
  type CodexBridgeStatus,
} from "../../api/settings";
import type { ModelConfig } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { AnalysisDefaults } from "./AnalysisDefaults";
import { CodexBridge } from "./CodexBridge";
import { CodexModelSelect } from "./CodexModelSelect";
import type { ReactNode } from "react";

export function ModelSettings({
  model,
  displaySettings,
  dataSettings,
  onUpdate,
  flash,
}: {
  model: ModelConfig;
  displaySettings?: ReactNode;
  dataSettings?: ReactNode;
  onUpdate: (m: ModelConfig) => void;
  flash: (s: string) => void;
}) {
  const [provider, setProvider] = useState<ModelConfig["provider"]>(
    model.provider,
  );
  const [baseUrl, setBaseUrl] = useState(model.baseUrl);
  const [modelName, setModelName] = useState(model.model);
  const [bridgeStatus, setBridgeStatus] = useState<CodexBridgeStatus | null>(
    null,
  );
  const [showKey, setShowKey] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [connectionResult, setConnectionResult] = useState("");
  const [error, setError] = useState("");
  const persist = async () => {
    setSaving(true);
    setError("");
    setConnectionResult("");
    try {
      const next = await saveModelConfig({
        provider,
        baseUrl,
        model: modelName,
        apiKey: apiKey || undefined,
      });
      onUpdate(next);
      setApiKey("");
      flash("模型配置已安全保存");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const testConnection = async () => {
    setTesting(true);
    setError("");
    setConnectionResult("");
    try {
      const result = await testModelConnection();
      setConnectionResult(`${result.model} · ${result.latencyMs} ms`);
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setTesting(false);
    }
  };

  const clearKey = async () => {
    if (!window.confirm("确认从账户密钥存储中移除模型 API Key？")) return;
    setSaving(true);
    setError("");
    setConnectionResult("");
    try {
      onUpdate(await deleteModelKey());
      flash("模型密钥已从账户密钥存储移除");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="page narrow settings-page">
      <PageHeader title="设置" />
      <nav className="settings-directory" aria-label="设置分区">
        <button
          type="button"
          onClick={() =>
            document
              .getElementById("model-settings")
              ?.scrollIntoView({ behavior: "smooth" })
          }
        >
          模型连接
        </button>
        <button
          type="button"
          onClick={() =>
            document
              .getElementById("analysis-settings")
              ?.scrollIntoView({ behavior: "smooth" })
          }
        >
          问答预设
        </button>
        <button
          type="button"
          onClick={() =>
            document
              .getElementById("market-settings")
              ?.scrollIntoView({ behavior: "smooth" })
          }
        >
          行情数据
        </button>
        <button
          type="button"
          onClick={() =>
            document
              .getElementById("display-settings")
              ?.scrollIntoView({ behavior: "smooth" })
          }
        >
          显示偏好
        </button>
        <button
          type="button"
          onClick={() =>
            document
              .getElementById("data-settings")
              ?.scrollIntoView({ behavior: "smooth" })
          }
        >
          导入数据
        </button>
      </nav>
      <section className="panel form-panel" id="model-settings">
        <div className="panel-title">
          <div>
            <h2>模型连接</h2>
          </div>
          <Badge tone={model.hasApiKey ? "success" : "neutral"}>
            {model.hasApiKey ? "已配置" : "未配置"}
          </Badge>
        </div>
        <fieldset className="holding-fields" disabled={saving || testing}>
          <div className="form-grid model-fields">
            <label>
              <span>模型接口</span>
              <select
                value={provider}
                onChange={(event) => {
                  const next = event.target.value as ModelConfig["provider"];
                  setProvider(next);
                  setBaseUrl(
                    next === "codex-bridge"
                      ? "codex://bridge"
                      : next === "anthropic"
                        ? "https://api.anthropic.com/v1"
                        : "https://api.openai.com/v1",
                  );
                  setModelName("");
                  setApiKey("");
                  setConnectionResult("");
                }}
              >
                <option value="openai-chat">
                  OpenAI 兼容 · Chat Completions
                </option>
                <option value="openai-responses">OpenAI Responses</option>
                <option value="anthropic">Anthropic Messages</option>
                <option value="codex-bridge">
                  本机桥接（ChatGPT 登录 / Codex）
                </option>
              </select>
            </label>
            {provider !== "codex-bridge" && (
              <label>
                <span>API Base URL</span>
                <input
                  aria-label="API Base URL"
                  value={baseUrl}
                  onChange={(e) => setBaseUrl(e.target.value)}
                  placeholder="https://api.openai.com/v1"
                />
              </label>
            )}
            {provider === "codex-bridge" ? (
              <CodexModelSelect
                value={modelName}
                onChange={setModelName}
                bridgeStatus={bridgeStatus}
              />
            ) : (
              <label>
                <span>模型名称</span>
                <input
                  value={modelName}
                  onChange={(e) => setModelName(e.target.value)}
                  placeholder="填写供应商提供的模型 ID"
                />
              </label>
            )}
            {provider !== "codex-bridge" && (
              <label>
                <span>API Key</span>
                <div className="secure-input">
                  <KeyRound size={16} />
                  <input
                    type={showKey ? "text" : "password"}
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder={
                      model.hasApiKey
                        ? "已保存在账户密钥存储；留空则不修改"
                        : "输入模型供应商密钥"
                    }
                  />
                  <button
                    type="button"
                    className="icon-button"
                    aria-label={showKey ? "隐藏模型密钥" : "显示模型密钥"}
                    onClick={() => setShowKey(!showKey)}
                  >
                    {showKey ? <EyeOff size={16} /> : <Eye size={16} />}
                  </button>
                </div>
              </label>
            )}
          </div>
        </fieldset>
        {provider === "codex-bridge" && (
          <CodexBridge
            onStatus={setBridgeStatus}
            disabled={saving || testing}
            onUpdate={(next) => {
              onUpdate(next);
              setProvider(next.provider);
              setBaseUrl(next.baseUrl);
              setModelName(next.model);
              setApiKey("");
              setConnectionResult("");
            }}
          />
        )}
        <Callout tone="info">
          <div>
            <strong>密钥与业务数据分离</strong>
            <p>
              {provider === "codex-bridge"
                ? "模型请求由配对电脑通过 ChatGPT 官方授权或本机 Codex 执行；只发送你确认的问答资料，登录与刷新凭据保留在本机。关闭桥接后问答会提示连接不可用，订阅额度不足不会自动切换到 API Key 计费。"
                : provider === "openai-chat"
                  ? "密钥不进入导出或 AI 资料。更换接口或地址需更新密钥。第三方服务可选择 OpenAI 兼容接口；Chat Completions 问答仅使用已选资料，不启用原生网页搜索。"
                  : "密钥不进入导出或 AI 资料。更换接口或地址需更新密钥；联网查找需模型支持原生网页搜索，连接测试仅验证普通调用。"}
            </p>
          </div>
        </Callout>
        {error && <Callout tone="danger">{error}</Callout>}
        {connectionResult && (
          <Callout tone="success">
            <span>
              <strong>连接成功</strong>
              {connectionResult}
            </span>
          </Callout>
        )}
        <div className="form-actions">
          <div className="key-actions">
            {model.hasApiKey && model.provider !== "codex-bridge" && (
              <button
                className="danger-text"
                onClick={clearKey}
                disabled={saving || testing}
              >
                <Trash2 size={14} />
                移除密钥
              </button>
            )}
            <button
              className="secondary"
              onClick={testConnection}
              disabled={saving || testing || !model.hasApiKey}
            >
              {testing ? (
                <LoaderCircle size={15} className="spin" />
              ) : (
                <Bot size={15} />
              )}
              测试已保存连接
            </button>
          </div>
          <button
            className="primary"
            onClick={persist}
            disabled={saving || testing || !baseUrl.trim() || !modelName.trim()}
          >
            <Save size={16} />
            {saving ? "保存中…" : "保存配置"}
          </button>
        </div>
      </section>
      <div id="analysis-settings">
        <AnalysisDefaults model={model} flash={flash} />
      </div>
      <section className="panel form-panel" id="market-settings">
        <h2>行情数据</h2>
        <p className="section-intro">
          所有账号共享同一证券的五年前复权日线，后台自动更新。持仓与趋势使用同一份有效收盘数据，并保留真实日期、来源和复权基准；成交和成本按原始记录保存。
        </p>
      </section>
      {displaySettings}
      {dataSettings}
    </div>
  );
}
