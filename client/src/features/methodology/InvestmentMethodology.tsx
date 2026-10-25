import { Callout } from "../../components/Callout";
import { useEffect, useState } from "react";
import { Save } from "lucide-react";
import {
  getInvestmentMethodology,
  saveInvestmentMethodology,
} from "../../api/settings";
import { PageHeader } from "../../components/PageHeader";
import { useRequestGuard } from "../../lib/useRequestGuard";
import type { InvestmentMethodology as Methodology } from "../../types";

export function InvestmentMethodology({
  flash,
  onSaved,
  onEdited,
}: {
  flash: (message: string) => void;
  onSaved: () => void;
  onEdited: () => void;
}) {
  const request = useRequestGuard();
  const [stored, setStored] = useState<Methodology | null>(null);
  const [prompt, setPrompt] = useState("");
  const [error, setError] = useState("");
  const [reload, setReload] = useState(0);
  const [savedAt, setSavedAt] = useState("");
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    const isCurrent = request.begin();
    void getInvestmentMethodology()
      .then((value) => {
        if (!isCurrent()) return;
        setStored(value);
        setPrompt(value.prompt);
        setError("");
      })
      .catch((nextError) => {
        if (isCurrent()) setError(String(nextError));
      });
  }, [reload, request]);

  const count = Array.from(prompt).length;
  const dirty = stored !== null && prompt !== stored.prompt;
  const valid =
    stored !== null && prompt.trim() !== "" && count <= stored.maxLength;
  const save = async () => {
    if (!dirty || !valid || saving) return;
    const isCurrent = request.begin();
    setSaving(true);
    setError("");
    try {
      const next = await saveInvestmentMethodology(prompt);
      if (!isCurrent()) return;
      setStored(next);
      setPrompt(next.prompt);
      setSavedAt(
        new Date().toLocaleTimeString("zh-CN", {
          hour: "2-digit",
          minute: "2-digit",
        }),
      );
      onSaved();
      flash("投资方法论已保存，将用于之后的每轮问答");
    } catch (nextError) {
      if (isCurrent()) setError(String(nextError));
    } finally {
      if (isCurrent()) setSaving(false);
    }
  };

  return (
    <div className="page narrow methodology-page">
      <PageHeader
        title="投资方法论"
        description="由你维护的问答提示词，每轮分析都会自动使用已保存的版本。"
      />
      <section className="panel form-panel methodology-editor">
        <div className="methodology-toolbar">
          <span>
            {count} / {stored?.maxLength ?? 4000} 字符 ·{" "}
            {dirty
              ? "有未保存的修改"
              : `已保存${savedAt ? ` · ${savedAt}` : ""}`}
          </span>
          <button
            className="primary"
            disabled={!dirty || !valid || saving}
            onClick={() => void save()}
          >
            <Save size={16} />
            {saving ? "保存中…" : "保存投资方法论"}
          </button>
        </div>
        <div className="methodology-layout">
          <div>
            <p className="section-intro">
              下面是当前使用的方法论全文。你可以直接修改或补充投资原则、决策顺序、研究重点与表达偏好。
            </p>
            {!stored && !error && <p role="status">正在读取投资方法论…</p>}
            {stored && (
              <fieldset className="holding-fields" disabled={saving}>
                <div className="form-grid single-column">
                  <label>
                    <span>投资方法论正文</span>
                    <textarea
                      rows={16}
                      value={prompt}
                      aria-describedby="methodology-help methodology-length"
                      onChange={(event) => {
                        setPrompt(event.target.value);
                        onEdited();
                      }}
                    />
                  </label>
                </div>
                <p
                  id="methodology-length"
                  className="sr-only"
                  aria-live="polite"
                >
                  {count} / {stored.maxLength} 字符
                </p>
                <p id="methodology-help" className="section-intro">
                  保存后用于之后的研究计划、方案比较、风险审查与回答。系统仍会检查数据质量、事实来源和输出格式；方法论不会修改你的持仓或财务档案。
                </p>
                {!valid && (
                  <p role="alert">
                    正文不能为空，且不能超过 {stored.maxLength} 字符。
                  </p>
                )}
              </fieldset>
            )}
            {error && (
              <Callout tone="danger">
                <span>{error}</span>
                {!stored && (
                  <button
                    className="secondary"
                    onClick={() => setReload((value) => value + 1)}
                  >
                    重新加载
                  </button>
                )}
              </Callout>
            )}
            <div className="form-actions">
              <button
                className="text-button"
                disabled={!stored || saving || prompt === stored.defaultPrompt}
                onClick={() => {
                  if (
                    stored &&
                    window.confirm(
                      "使用初始范本会覆盖编辑区当前内容，保存后才会生效。继续？",
                    )
                  ) {
                    setPrompt(stored.defaultPrompt);
                    onEdited();
                  }
                }}
              >
                使用初始范本
              </button>
            </div>
            <p className="section-intro">
              初始范本仅填入编辑区，点击保存后才会生效。
            </p>
          </div>
          <aside className="methodology-tips">
            <h3>写作提示</h3>
            <h4>投资原则</h4>
            <p>写清风险边界和不可做的事。</p>
            <h4>决策顺序</h4>
            <p>先核对哪些事实，再比较哪些方案。</p>
            <h4>研究重点</h4>
            <p>列出关心的行业、资产与证伪线索。</p>
            <h4>表达偏好</h4>
            <p>说明希望如何解释依据、局限与行动。</p>
          </aside>
        </div>
      </section>
    </div>
  );
}
