import { useEffect, useState } from "react";
import { AlertTriangle, LoaderCircle, Save } from "lucide-react";
import { getAnalysisSettings, saveAnalysisSettings } from "../../api/settings";
import type {
  AnalysisSettings,
  ContextSelection,
  ModelConfig,
} from "../../types";

const contextOptions: [keyof ContextSelection, string][] = [
  ["includeProfile", "财务档案"],
  ["includeHoldings", "持仓明细"],
  ["includeEvidence", "研究证据与历史搜索资料"],
];

export function AnalysisDefaults({
  model,
  flash,
}: {
  model: ModelConfig;
  flash: (message: string) => void;
}) {
  const [settings, setSettings] = useState<AnalysisSettings | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [reload, setReload] = useState(0);
  useEffect(() => {
    let mounted = true;
    setError("");
    void getAnalysisSettings()
      .then((value) => {
        if (mounted) setSettings(value);
      })
      .catch((nextError) => {
        if (mounted) setError(String(nextError));
      });
    return () => {
      mounted = false;
    };
  }, [reload]);

  const save = async () => {
    if (!settings || saving) return;
    setSaving(true);
    setError("");
    try {
      setSettings(await saveAnalysisSettings(settings));
      flash("问答预设已保存，之后每次提问自动使用");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="panel form-panel analysis-defaults">
      <div className="panel-title">
        <h2>问答预设</h2>
      </div>
      <p className="section-intro">
        一次配置并保存，之后每次提问自动将所选资料发送给模型。资料内容使用最新记录，无需逐次确认。
      </p>
      {!settings && !error && <p role="status">正在读取问答预设…</p>}
      {settings && (
        <fieldset className="holding-fields" disabled={saving}>
          <h3>自动输入的资料</h3>
          <div className="analysis-default-options">
            {contextOptions.map(([key, label]) => (
              <label key={key}>
                <input
                  type="checkbox"
                  checked={settings.contextSelection[key]}
                  onChange={(event) =>
                    setSettings({
                      ...settings,
                      contextSelection: {
                        ...settings.contextSelection,
                        [key]: event.target.checked,
                      },
                    })
                  }
                />
                {label}
              </label>
            ))}
          </div>
          <h3>分析选项</h3>
          <div className="analysis-default-options">
            {(
              [
                ["reflect", "反思检查"],
                ["exploreAlternatives", "比较备选方案"],
              ] as const
            ).map(([key, label]) => (
              <label key={key}>
                <input
                  type="checkbox"
                  checked={settings[key]}
                  onChange={(event) =>
                    setSettings({ ...settings, [key]: event.target.checked })
                  }
                />
                {label}
              </label>
            ))}
            <label>
              <input
                type="checkbox"
                checked={settings.webSearch}
                onChange={(event) =>
                  setSettings({ ...settings, webSearch: event.target.checked })
                }
              />
              联网查找相关资料
            </label>
          </div>
          <p className="section-intro">
            问答固定使用深度分析，并按照已保存的资料范围准备上下文。
          </p>
          <p className="section-intro">
            {model.provider === "openai-chat"
              ? "当前 Chat Completions 接口不支持原生网页搜索，联网选项暂不生效。"
              : "启用联网后，模型供应商会自动搜索网页，可能产生额外费用。"}
          </p>
          <p className="section-intro">
            固定投资方法论和当前对话的上一轮问答摘要会自动附上；新对话不携带上一轮摘要。模型密钥不会作为资料发送。
          </p>
        </fieldset>
      )}
      {error && (
        <div className="error-box" role="alert">
          <AlertTriangle size={18} />
          {error}
          {!settings && (
            <button
              className="text-button"
              onClick={() => setReload((value) => value + 1)}
            >
              重试
            </button>
          )}
        </div>
      )}
      <div className="form-actions">
        <button
          className="primary"
          disabled={!settings || saving}
          onClick={() => void save()}
        >
          {saving ? (
            <LoaderCircle size={16} className="spin" />
          ) : (
            <Save size={16} />
          )}
          {saving ? "保存中…" : "保存问答预设"}
        </button>
      </div>
    </section>
  );
}
