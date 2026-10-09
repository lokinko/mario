import { isLocalDev } from "../../lib/service";
import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Bot,
  Check,
  Cloud,
  KeyRound,
  LoaderCircle,
  LockKeyhole,
  Save,
  Trash2,
} from "lucide-react";
import {
  deleteModelKey,
  readCodexCredentials,
  deleteSecurityPriceKey,
  getSecurityPrice,
  getSecurityPriceConfig,
  saveModelConfig,
  saveSecurityPriceConfig,
  testModelConnection,
} from "../../api";
import type { ModelConfig, SecurityPriceConfig } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { localDateValue } from "../../lib/dates";

export function ModelSettings({
  model,
  onUpdate,
  flash,
}: {
  model: ModelConfig;
  onUpdate: (m: ModelConfig) => void;
  flash: (s: string) => void;
}) {
  const [provider, setProvider] = useState<ModelConfig["provider"]>(
    model.provider === "openai-compatible"
      ? "openai-responses"
      : model.provider,
  );
  const [baseUrl, setBaseUrl] = useState(model.baseUrl);
  const [modelName, setModelName] = useState(model.model);
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [connectionResult, setConnectionResult] = useState("");
  const [error, setError] = useState("");
  const [securityConfig, setSecurityConfig] =
    useState<SecurityPriceConfig | null>(null);
  const [securityApiKey, setSecurityApiKey] = useState("");
  const [marketBusy, setMarketBusy] = useState(false);
  const [marketResult, setMarketResult] = useState("");
  const [marketError, setMarketError] = useState("");

  useEffect(() => {
    let active = true;
    void getSecurityPriceConfig()
      .then((value) => {
        if (active) setSecurityConfig(value);
      })
      .catch((nextError) => {
        if (active) setMarketError(String(nextError));
      });
    return () => {
      active = false;
    };
  }, []);

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

  const readCodex = async () => {
    setSaving(true);
    setError("");
    setConnectionResult("");
    try {
      const next = await readCodexCredentials();
      onUpdate(next);
      setProvider(next.provider);
      setBaseUrl(next.baseUrl);
      setModelName(next.model);
      setApiKey("");
      setConnectionResult("Codex 凭证读取成功，已启用 " + next.model);
      flash("已启用本机 Codex 登录");
    } catch (error) {
      setError("Codex 凭证读取失败：" + String(error));
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

  const persistSecurityKey = async () => {
    if (!securityApiKey.trim()) return;
    setMarketBusy(true);
    setMarketError("");
    setMarketResult("");
    try {
      setSecurityConfig(await saveSecurityPriceConfig(securityApiKey));
      setSecurityApiKey("");
      flash("行情密钥已安全保存");
    } catch (nextError) {
      setMarketError(String(nextError));
    } finally {
      setMarketBusy(false);
    }
  };

  const testSecurityConnection = async () => {
    setMarketBusy(true);
    setMarketError("");
    setMarketResult("");
    try {
      const quote = await getSecurityPrice("AAPL", localDateValue(new Date()));
      setMarketResult(
        `${quote.symbol} · ${quote.observedOn} · ${quote.close} ${quote.currency} · ${quote.exchange || quote.micCode}`,
      );
    } catch (nextError) {
      setMarketError(String(nextError));
    } finally {
      setMarketBusy(false);
    }
  };

  const clearSecurityKey = async () => {
    if (!window.confirm("确认从账户密钥存储中移除 Twelve Data API Key？"))
      return;
    setMarketBusy(true);
    setMarketError("");
    setMarketResult("");
    try {
      setSecurityConfig(await deleteSecurityPriceKey());
      flash("行情密钥已从账户密钥存储移除");
    } catch (nextError) {
      setMarketError(String(nextError));
    } finally {
      setMarketBusy(false);
    }
  };

  return (
    <div className="page narrow settings-page">
      <PageHeader
        title="模型与隐私"
        description="密钥按账号加密保存，资料在确认后发送给模型。"
      />
      <section className="panel form-panel">
        <div className="panel-title">
          <div>
            <h2>模型连接</h2>
          </div>
          <div className={`status-dot ${model.hasApiKey ? "connected" : ""}`}>
            {model.hasApiKey ? "已配置" : "未配置"}
          </div>
        </div>
        {isLocalDev() && (
          <>
            <button
              className="secondary"
              onClick={() => void readCodex()}
              disabled={saving || testing}
            >
              {saving ? (
                <LoaderCircle size={16} className="spin" />
              ) : (
                <KeyRound size={16} />
              )}
              一键读取 Codex 凭证
            </button>
            <p className="section-intro">
              使用本机已登录的 Codex，无需复制密钥。
            </p>
          </>
        )}
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
                    next === "codex"
                      ? "codex://local"
                      : next === "anthropic"
                        ? "https://api.anthropic.com/v1"
                        : "https://api.openai.com/v1",
                  );
                  setModelName("");
                  setApiKey("");
                  setConnectionResult("");
                }}
              >
                <option value="openai-responses">OpenAI Responses</option>
                <option value="anthropic">Anthropic Messages</option>
                {isLocalDev() && <option value="codex">Codex 本机登录</option>}
              </select>
            </label>
            <label>
              <span>API Base URL</span>
              <input
                value={baseUrl}
                disabled={provider === "codex"}
                onChange={(e) => setBaseUrl(e.target.value)}
                placeholder="https://api.openai.com/v1"
              />
            </label>
            <label>
              <span>模型名称</span>
              <input
                value={modelName}
                onChange={(e) => setModelName(e.target.value)}
                placeholder={
                  provider === "anthropic"
                    ? "填写支持网页搜索的 Claude 模型 ID"
                    : "填写支持网页搜索的 OpenAI 模型 ID"
                }
              />
            </label>
            {provider !== "codex" && (
              <label>
                <span>API Key</span>
                <div className="secure-input">
                  <KeyRound size={16} />
                  <input
                    type="password"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder={
                      model.hasApiKey
                        ? "已保存在账户密钥存储；留空则不修改"
                        : "输入模型供应商密钥"
                    }
                  />
                </div>
              </label>
            )}
          </div>
        </fieldset>
        <div className="privacy-note">
          <LockKeyhole size={18} />
          <div>
            <strong>密钥与业务数据分离</strong>
            <p>
              {provider === "codex"
                ? "凭证由 Codex 保管和刷新，不复制到 mario。连接测试不验证网页搜索能力。"
                : "密钥不进入导出或 AI 资料。更换接口或地址需更新密钥；模型需支持原生网页搜索，连接测试仅验证普通调用。"}
            </p>
          </div>
        </div>
        {error && (
          <div className="error-box" role="alert">
            <AlertTriangle size={18} />
            {error}
          </div>
        )}
        {connectionResult && (
          <div className="connection-success" role="status">
            <Check size={16} />
            <span>
              <strong>
                {model.provider === "codex" ? "Codex 状态" : "连接成功"}
              </strong>
              {connectionResult}
            </span>
          </div>
        )}
        <div className="form-actions">
          <div className="key-actions">
            {model.hasApiKey && model.provider !== "codex" && (
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
      <details className="panel form-panel optional-settings">
        <summary className="panel-title">
          <div>
            <h2>行情数据 · Twelve Data</h2>
          </div>
          <div
            className={`status-dot ${securityConfig?.hasApiKey ? "connected" : ""}`}
          >
            {securityConfig?.hasApiKey ? "已配置" : "未配置"}
          </div>
        </summary>
        <p className="section-intro">
          仅在主动查询时连接；密钥按账号加密保存，不发送给 AI。
        </p>
        <div className="form-grid single-column">
          <label>
            <span>Twelve Data API Key</span>
            <div className="secure-input">
              <KeyRound size={16} />
              <input
                disabled={marketBusy}
                type="password"
                value={securityApiKey}
                onChange={(event) => setSecurityApiKey(event.target.value)}
                placeholder={
                  securityConfig?.hasApiKey
                    ? "已保存在账户密钥存储；留空则不修改"
                    : "输入个人 Twelve Data 密钥"
                }
              />
            </div>
          </label>
        </div>
        <details className="inline-help">
          <summary>数据来源与许可</summary>
          <p>
            保存来源、观察日和未复权收盘价。使用范围和额度以你的 Twelve Data
            账户条款为准。
          </p>
          <p>
            <a
              href="https://twelvedata.com/docs/market-data/time-series"
              target="_blank"
              rel="noreferrer"
            >
              接口方法
            </a>{" "}
            ·{" "}
            <a
              href="https://twelvedata.com/pricing"
              target="_blank"
              rel="noreferrer"
            >
              额度
            </a>{" "}
            ·{" "}
            <a
              href="https://twelvedata.com/terms"
              target="_blank"
              rel="noreferrer"
            >
              许可条款
            </a>
          </p>
        </details>
        {marketError && (
          <div className="error-box" role="alert">
            <AlertTriangle size={18} />
            {marketError}
          </div>
        )}
        {marketResult && (
          <div className="connection-success" role="status">
            <Check size={16} />
            <span>
              <strong>价格连接成功</strong>
              {marketResult}
            </span>
          </div>
        )}
        <div className="form-actions">
          <div className="key-actions">
            {securityConfig?.hasApiKey && (
              <button
                className="danger-text"
                onClick={clearSecurityKey}
                disabled={marketBusy}
              >
                <Trash2 size={14} />
                移除密钥
              </button>
            )}
            <button
              className="secondary"
              onClick={testSecurityConnection}
              disabled={marketBusy || !securityConfig?.hasApiKey}
            >
              {marketBusy ? (
                <LoaderCircle size={15} className="spin" />
              ) : (
                <Cloud size={15} />
              )}
              测试 AAPL 日线
            </button>
          </div>
          <button
            className="primary"
            onClick={persistSecurityKey}
            disabled={marketBusy || !securityApiKey.trim()}
          >
            <Save size={16} />
            保存行情密钥
          </button>
        </div>
      </details>
    </div>
  );
}
