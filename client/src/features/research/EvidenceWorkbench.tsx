import { useEffect, useState } from "react";
import { AlertTriangle, Database, Save, Sparkles } from "lucide-react";
import {
  getResearchEvidence,
  saveResearchEvidence,
  setResearchEvidenceStatus,
} from "../../api";
import type { ResearchEvidence, ResearchEvidenceInput } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { localDateValue } from "../../lib/dates";
import { View } from "../../app/navigation";

export function emptyEvidence(): ResearchEvidenceInput {
  return {
    assetName: "",
    title: "",
    publisher: "",
    sourceUrl: "",
    sourceTier: "一手来源",
    evidenceType: "公司披露",
    stance: "背景",
    asOfDate: localDateValue(new Date()),
    claim: "",
    notes: "",
  };
}

export function sourceHost(value: string) {
  try {
    return new URL(value).hostname;
  } catch {
    return value;
  }
}

export function EvidenceWorkbench({
  navigate,
  flash,
}: {
  navigate: (v: View) => void;
  flash: (s: string) => void;
}) {
  const [draft, setDraft] = useState<ResearchEvidenceInput>(emptyEvidence);
  const [items, setItems] = useState<ResearchEvidence[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const refresh = async () => {
    try {
      setItems(await getResearchEvidence());
      setError("");
    } catch (nextError) {
      setError(String(nextError));
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const active = items.filter((item) => item.active);
  const primary = active.filter((item) => item.sourceTier === "一手来源");
  const counter = active.filter((item) => item.stance === "反驳");
  const oneYearAgo = new Date();
  oneYearAgo.setFullYear(oneYearAgo.getFullYear() - 1);
  const aging = active.filter(
    (item) => new Date(`${item.asOfDate}T00:00:00`) < oneYearAgo,
  );

  const persist = async () => {
    if (
      !draft.assetName ||
      !draft.title ||
      !draft.publisher ||
      !draft.sourceUrl ||
      !draft.asOfDate ||
      !draft.claim
    )
      return;
    setSaving(true);
    setError("");
    try {
      await saveResearchEvidence(draft);
      setDraft(emptyEvidence());
      await refresh();
      flash("研究证据已保存在本机，原始内容将保持不变");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const toggleStatus = async (item: ResearchEvidence) => {
    setSaving(true);
    setError("");
    try {
      await setResearchEvidenceStatus(item.id, !item.active);
      await refresh();
      flash(
        item.active ? "证据已归档，不再进入 AI 检索" : "证据已恢复为有效状态",
      );
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const startEvidenceAnalysis = () => {
    const assets = [...new Set(active.map((item) => item.assetName))].join(
      "、",
    );
    window.sessionStorage.setItem(
      "mario.advisorQuestion",
      `请基于我保存的带来源研究证据，审查${assets || "当前组合"}的投资假设：区分一手事实、二手解释与未知项，优先寻找反方证据，只引用载荷中实际存在的 HTTPS 来源，并给出下一步需要补齐的证据。`,
    );
    navigate("advisor");
  };

  return (
    <div className="page narrow">
      <PageHeader
        eyebrow="方法论 · 研究层"
        title="观点之前，先建立证据"
        description="保存事实出处、资料日期与反方证据；AI 只能引用你确认发送的记录。"
        action={
          <button
            className="primary"
            onClick={startEvidenceAnalysis}
            disabled={!active.length}
          >
            <Sparkles size={16} />
            用证据开始分析
          </button>
        }
      />
      <section className="review-metrics">
        <article>
          <span>有效证据</span>
          <strong>{active.length}</strong>
          <small>{items.length - active.length} 条已归档</small>
        </article>
        <article>
          <span>一手来源</span>
          <strong>{primary.length}</strong>
          <small>披露、监管或原始数据</small>
        </article>
        <article>
          <span>反方证据</span>
          <strong>{counter.length}</strong>
          <small>避免只收集支持材料</small>
        </article>
        <article>
          <span>超过一年</span>
          <strong className={aging.length ? "warning-text" : ""}>
            {aging.length}
          </strong>
          <small>过期不等于错误，但需要复核</small>
        </article>
      </section>

      <section className="panel evidence-form">
        <div className="panel-title">
          <div>
            <span>证据账本</span>
            <h2>记录一条可追溯事实</h2>
          </div>
          <Database size={21} className="muted-icon" />
        </div>
        <div className="evidence-entry-layout">
          <div className="form-grid">
            <label>
              <span>关联资产或主题</span>
              <input
                maxLength={120}
                value={draft.assetName}
                onChange={(e) =>
                  setDraft({ ...draft, assetName: e.target.value })
                }
                placeholder="例如：全球指数、黄金、某家公司"
              />
            </label>
            <label>
              <span>资料标题</span>
              <input
                maxLength={300}
                value={draft.title}
                onChange={(e) => setDraft({ ...draft, title: e.target.value })}
                placeholder="使用来源页面的准确标题"
              />
            </label>
            <label>
              <span>发布方</span>
              <input
                maxLength={200}
                value={draft.publisher}
                onChange={(e) =>
                  setDraft({ ...draft, publisher: e.target.value })
                }
                placeholder="公司、监管机构或研究机构"
              />
            </label>
            <label>
              <span>HTTPS 来源链接</span>
              <input
                type="url"
                maxLength={2048}
                value={draft.sourceUrl}
                onChange={(e) =>
                  setDraft({ ...draft, sourceUrl: e.target.value })
                }
                placeholder="https://…（不要包含访问令牌）"
              />
            </label>
            <label>
              <span>来源层级</span>
              <select
                value={draft.sourceTier}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    sourceTier: e.target
                      .value as ResearchEvidenceInput["sourceTier"],
                  })
                }
              >
                <option>一手来源</option>
                <option>二手研究</option>
                <option>媒体报道</option>
              </select>
            </label>
            <label>
              <span>证据类型</span>
              <select
                value={draft.evidenceType}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    evidenceType: e.target
                      .value as ResearchEvidenceInput["evidenceType"],
                  })
                }
              >
                {[
                  "公司披露",
                  "监管文件",
                  "数据发布",
                  "研究报告",
                  "新闻",
                  "其他",
                ].map((value) => (
                  <option key={value}>{value}</option>
                ))}
              </select>
            </label>
            <label>
              <span>与当前假设的关系</span>
              <select
                value={draft.stance}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    stance: e.target.value as ResearchEvidenceInput["stance"],
                  })
                }
              >
                <option>支持</option>
                <option>反驳</option>
                <option>背景</option>
              </select>
            </label>
            <label>
              <span>资料日期</span>
              <input
                type="date"
                max={localDateValue(new Date())}
                value={draft.asOfDate}
                onChange={(e) =>
                  setDraft({ ...draft, asOfDate: e.target.value })
                }
              />
            </label>
            <label className="span-2">
              <span>这条来源实际支持什么事实？</span>
              <textarea
                maxLength={4000}
                value={draft.claim}
                onChange={(e) => setDraft({ ...draft, claim: e.target.value })}
                placeholder="只记录来源能够直接支持的内容，不写买卖结论。"
              />
            </label>
            <label className="span-2">
              <span>限制与待核实项（可选）</span>
              <textarea
                maxLength={4000}
                value={draft.notes}
                onChange={(e) => setDraft({ ...draft, notes: e.target.value })}
                placeholder="口径差异、样本限制、尚未核验的解释。"
              />
            </label>
          </div>
          <aside className="evidence-guide">
            <strong>来源不是结论</strong>
            <p>“一手来源”表示离原始事实更近，不代表内容完整或投资判断正确。</p>
            <ol>
              <li>优先保存公司披露、监管文件和原始数据。</li>
              <li>支持材料与反方材料分开记录。</li>
              <li>错误记录应归档并重新建立，不覆盖旧证据。</li>
              <li>mario 当前不自动抓取或核验链接内容。</li>
            </ol>
          </aside>
        </div>
        {error && (
          <div className="error-box">
            <AlertTriangle size={18} />
            {error}
          </div>
        )}
        <div className="form-actions">
          <p>保存后内容不可编辑；归档是可恢复操作。</p>
          <button
            className="primary"
            onClick={persist}
            disabled={
              saving ||
              !draft.assetName ||
              !draft.title ||
              !draft.publisher ||
              !draft.sourceUrl ||
              !draft.asOfDate ||
              !draft.claim
            }
          >
            <Save size={16} />
            保存证据
          </button>
        </div>
      </section>

      <section className="panel evidence-library">
        <div className="panel-title">
          <div>
            <span>本地证据库</span>
            <h2>检查来源结构，而不是累计观点数量</h2>
          </div>
          <span className="history-count">{items.length} 条</span>
        </div>
        {items.length === 0 && (
          <div className="empty">
            还没有研究证据。先从一条可以打开、可以标注日期的一手来源开始。
          </div>
        )}
        <div className="evidence-list">
          {items.map((item) => (
            <article key={item.id} className={item.active ? "" : "inactive"}>
              <div className="evidence-head">
                <div>
                  <span className={`stance-${item.stance}`}>{item.stance}</span>
                  <strong>{item.assetName}</strong>
                  <em>{item.sourceTier}</em>
                </div>
                <small>{item.asOfDate}</small>
              </div>
              <h3>{item.title}</h3>
              <p>{item.claim}</p>
              {item.notes && (
                <small className="evidence-notes">限制：{item.notes}</small>
              )}
              <footer>
                <a href={item.sourceUrl} target="_blank" rel="noreferrer">
                  {item.publisher} · {sourceHost(item.sourceUrl)}
                </a>
                <button
                  className="text-button"
                  disabled={saving}
                  onClick={() => toggleStatus(item)}
                >
                  {item.active ? "归档" : "恢复"}
                </button>
              </footer>
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}
