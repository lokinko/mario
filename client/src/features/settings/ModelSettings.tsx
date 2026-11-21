import { Tabs } from "antd";
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
import type { ModelConfig } from "../../types/ai";
import { PageHeader } from "../../components/PageHeader";
import { AccountModelSelect } from "./AccountModelSelect";
import { CodexBridge } from "./CodexBridge";
import { CodexModelSelect } from "./CodexModelSelect";
import { ServerModelSelect } from "./ServerModelSelect";
import type { ReactNode } from "react";

function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function ModelSettings({
  model,
  displaySettings,
  accountSettings,
  onUpdate,
  onSaved,
  flash,
}: {
  model: ModelConfig;
  displaySettings?: ReactNode;
  accountSettings?: ReactNode;
  onUpdate: (m: ModelConfig) => void;
  onSaved?: () => void;
  flash: (s: string) => void;
}) {
  const [source, setSource] = useState(
    model.source ?? (model.serverModel ? "server" : "account"),
  );
  const usingServer = source === "server";
  const [serverModelName, setServerModelName] = useState(
    model.source === "server" ? model.model : (model.serverModel ?? ""),
  );
  const [provider, setProvider] = useState<ModelConfig["provider"]>(
    model.provider,
  );
  const [baseUrl, setBaseUrl] = useState(model.baseUrl);
  const [modelName, setModelName] = useState(
    model.source === "server" ? "" : model.model,
  );
  const [bridgeStatus, setBridgeStatus] = useState<CodexBridgeStatus | null>(
    null,
  );
  const [showKey, setShowKey] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [connectionResult, setConnectionResult] = useState("");
  const [error, setError] = useState("");
  const hasSavedBinding =
    model.source !== "server" &&
    model.hasApiKey &&
    model.provider === provider &&
    model.baseUrl.trim().replace(/\/+$/, "") ===
      baseUrl.trim().replace(/\/+$/, "");
  const persist = async () => {
    setSaving(true);
    setError("");
    setConnectionResult("");
    try {
      const next = await saveModelConfig({
        source,
        provider: usingServer ? "openai-responses" : provider,
        baseUrl: usingServer ? "https://api.openai.com/v1" : baseUrl,
        model: usingServer ? serverModelName : modelName,
        apiKey: usingServer ? undefined : apiKey || undefined,
      });
      onUpdate(next);
      setApiKey("");
      onSaved?.();
      flash("模型配置已保存");
    } catch (nextError) {
      setError(formatError(nextError));
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
      setError(formatError(nextError));
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
      onSaved?.();
      flash("模型密钥已移除");
    } catch (nextError) {
      setError(formatError(nextError));
    } finally {
      setSaving(false);
    }
  };

  const modelPanel = (
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
        {(model.serverModel || model.source === "server") && (
          <label>
            <span>调用方式</span>
            <select
              value={source}
              onChange={(event) => {
                setSource(event.target.value as "account" | "server");
                setApiKey("");
                setConnectionResult("");
              }}
            >
              <option value="server" disabled={!model.serverModel}>
                站点提供的 OpenAI（默认）
              </option>
              <option value="account">个人模型或本机桥接</option>
            </select>
          </label>
        )}
        {usingServer ? (
          model.serverModel ? (
            <ServerModelSelect
              value={serverModelName}
              onChange={setServerModelName}
              defaultModel={model.serverModel}
              disabled={saving || testing}
            />
          ) : (
            <p>站点 OpenAI 暂未启用，请联系管理员或切换个人模型。</p>
          )
        ) : (
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
                      model.source !== "server" && model.hasApiKey
                        ? "已保存，留空不修改"
                        : "输入 API Key"
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
            {provider === "codex-bridge" ? (
              <CodexModelSelect
                value={modelName}
                onChange={setModelName}
                bridgeStatus={bridgeStatus}
              />
            ) : (
              <AccountModelSelect
                provider={provider}
                baseUrl={baseUrl}
                apiKey={apiKey}
                value={modelName}
                savedModel={model.source === "server" ? "" : model.model}
                hasSavedBinding={hasSavedBinding}
                disabled={saving || testing}
                onChange={setModelName}
              />
            )}
          </div>
        )}
      </fieldset>
      {!usingServer && provider === "codex-bridge" && (
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
          {model.source !== "server" &&
            model.hasApiKey &&
            model.provider !== "codex-bridge" && (
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
          disabled={
            saving ||
            testing ||
            (usingServer
              ? !model.serverModel || !serverModelName.trim()
              : !baseUrl.trim() || !modelName.trim())
          }
        >
          <Save size={16} />
          {saving ? "保存中…" : "保存配置"}
        </button>
      </div>
    </section>
  );

  const tabItems = [
    { key: "model", label: "模型连接", children: modelPanel },
    ...(accountSettings
      ? [{ key: "account", label: "账号与数据", children: accountSettings }]
      : []),
    ...(displaySettings
      ? [{ key: "display", label: "显示偏好", children: displaySettings }]
      : []),
  ];

  return (
    <div className="page narrow settings-page">
      <PageHeader title="设置" />
      {tabItems.length > 1 ? (
        <Tabs aria-label="设置分区" items={tabItems} />
      ) : (
        modelPanel
      )}
    </div>
  );
}
