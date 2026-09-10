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
        provider: "openai-compatible",
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
    if (!window.confirm("确认从系统钥匙串中移除模型 API Key？")) return;
    setSaving(true);
    setError("");
    setConnectionResult("");
    try {
      onUpdate(await deleteModelKey());
      flash("模型密钥已从系统钥匙串移除");
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
    if (!window.confirm("确认从系统钥匙串中移除 Twelve Data API Key？")) return;
    setMarketBusy(true);
    setMarketError("");
    setMarketResult("");
    try {
      setSecurityConfig(await deleteSecurityPriceKey());
      flash("行情密钥已从系统钥匙串移除");
    } catch (nextError) {
      setMarketError(String(nextError));
    } finally {
      setMarketBusy(false);
    }
  };

  return (
    <div className="page narrow">
      <PageHeader
        eyebrow="模型与外部数据"
        title="模型可以替换，方法论保持稳定"
        description="AI 与行情服务分别授权；密钥留在设备端，投资数据只按明确动作发送。"
      />
      <section className="panel form-panel">
        <div className="panel-title">
          <div>
            <span>OpenAI-compatible</span>
            <h2>模型连接</h2>
          </div>
          <div className={`status-dot ${model.hasApiKey ? "connected" : ""}`}>
            {model.hasApiKey ? "已配置" : "未配置"}
          </div>
        </div>
        <div className="form-grid single-column">
          <label>
            <span>API Base URL</span>
            <input
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://api.openai.com/v1"
            />
          </label>
          <label>
            <span>模型名称</span>
            <input
              value={modelName}
              onChange={(e) => setModelName(e.target.value)}
              placeholder="gpt-4.1-mini"
            />
          </label>
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
                    ? "已保存在系统钥匙串；留空则不修改"
                    : "输入模型供应商密钥"
                }
              />
            </div>
          </label>
        </div>
        <div className="privacy-note">
          <LockKeyhole size={18} />
          <div>
            <strong>密钥与业务数据分离</strong>
            <p>
              密钥由操作系统钥匙串托管；本地 SQLite
              数据库只保存提供商、地址和模型名称。
            </p>
          </div>
        </div>
        {error && (
          <div className="error-box">
            <AlertTriangle size={18} />
            {error}
          </div>
        )}
        {connectionResult && (
          <div className="connection-success">
            <Check size={16} />
            <span>
              <strong>连接成功</strong>
              {connectionResult}
            </span>
          </div>
        )}
        <div className="form-actions">
          <div className="key-actions">
            {model.hasApiKey && (
              <button
                className="danger-text"
                onClick={clearKey}
                disabled={saving}
              >
                <Trash2 size={14} />
                移除密钥
              </button>
            )}
            <button
              className="secondary"
              onClick={testConnection}
              disabled={testing || !model.hasApiKey}
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
            disabled={saving || !baseUrl || !modelName}
          >
            <Save size={16} />
            保存配置
          </button>
        </div>
      </section>
      <section className="panel form-panel">
        <div className="panel-title">
          <div>
            <span>可审计证券价格</span>
            <h2>Twelve Data 日收盘价</h2>
          </div>
          <div
            className={`status-dot ${securityConfig?.hasApiKey ? "connected" : ""}`}
          >
            {securityConfig?.hasApiKey ? "已配置" : "未配置"}
          </div>
        </div>
        <p className="section-intro">
          只在你主动查询持仓估值时调用。服务端使用 Authorization
          请求头，密钥不进入浏览器地址、SQLite、AI 上下文或云端同步包。
        </p>
        <div className="form-grid single-column">
          <label>
            <span>Twelve Data API Key</span>
            <div className="secure-input">
              <KeyRound size={16} />
              <input
                type="password"
                value={securityApiKey}
                onChange={(event) => setSecurityApiKey(event.target.value)}
                placeholder={
                  securityConfig?.hasApiKey
                    ? "已保存在系统钥匙串；留空则不修改"
                    : "输入个人 Twelve Data 密钥"
                }
              />
            </div>
          </label>
        </div>
        <div className="privacy-note">
          <LockKeyhole size={18} />
          <div>
            <strong>来源透明，许可归用户账户</strong>
            <p>
              mario 保存代码、币种、交易所、观察日和未复权收盘价口径。免费 Basic
              方案当前有每分钟与每日额度；个人方案仅适合个人、内部和非商业用途。
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
          </div>
        </div>
        {marketError && (
          <div className="error-box">
            <AlertTriangle size={18} />
            {marketError}
          </div>
        )}
        {marketResult && (
          <div className="connection-success">
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
      </section>
      <section className="architecture-grid">
        <article>
          <span>01</span>
          <strong>确定性规则层</strong>
          <p>现金流、集中度、期限错配等风险无需调用模型。</p>
        </article>
        <article>
          <span>02</span>
          <strong>上下文构建层</strong>
          <p>只选择完成当前任务所需的本地数据。</p>
        </article>
        <article>
          <span>03</span>
          <strong>可替换编排层</strong>
          <p>记忆、检索、探索和反思都是独立阶段。</p>
        </article>
      </section>
    </div>
  );
}
