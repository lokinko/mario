import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { Badge } from "../../components/Badge";
import { DataStamp } from "../../components/DataStamp";
import { GenerationSteps } from "./GenerationSteps";
import { reportSources } from "./ReportSources";
import { PageHeader } from "../../components/PageHeader";
import { SERVER_DATA_UPDATED } from "../../lib/syncEvents";
import { useEffect, useRef, useState } from "react";
import {
  ChevronRight,
  Globe,
  LoaderCircle,
  Send,
  SlidersHorizontal,
  Square,
} from "lucide-react";
import {
  getAnalysis,
  getAnalysisHistory,
  previewAnalysis,
  runAnalysis,
} from "../../api/analysis";
import { getAnalysisSettings, saveAnalysisSettings } from "../../api/settings";
import { dreamMemory, getMemory } from "../../api/memory";
import { MemoryPanel } from "./MemoryPanel";
import { ContextPreview } from "./ContextPreview";
import type {
  AnalysisRequest,
  AnalysisSettings,
  AnalysisPreview,
  AnalysisResult,
  AnalysisHistoryItem,
  ContextSelection,
  ModelConfig,
  StoredAnalysis,
} from "../../types/ai";
import { ModelReport } from "./ModelReport";
import { previousTurn } from "./previousTurn";
import { useRequestGuard } from "../../lib/useRequestGuard";

type Turn = { question: string; result: AnalysisResult | StoredAnalysis };

const defaultAnalysisSettings: AnalysisSettings = {
  workflow: "council",
  webSearch: false,
  contextSelection: {
    includeProfile: true,
    includeHoldings: true,
    includeEvidence: true,
    includeMemory: true,
  },
};

const contextToggleItems: {
  key: keyof ContextSelection;
  label: string;
}[] = [
  { key: "includeProfile", label: "财务档案" },
  { key: "includeHoldings", label: "当前持仓" },
  { key: "includeEvidence", label: "研究资料" },
  { key: "includeMemory", label: "长期记忆" },
];

function buildRequest(
  settings: AnalysisSettings,
  model: ModelConfig,
  question: string,
  turns: readonly Turn[],
): AnalysisRequest {
  return {
    ...settings,
    webSearch: settings.webSearch && model.provider !== "openai-chat",
    question,
    previousTurn: previousTurn(turns),
  };
}

export function Advisor({
  model,
  active = true,
  onOpenSettings,
  pendingQuestion,
  onQuestionAccepted,
}: {
  model: ModelConfig;
  active?: boolean;
  onOpenSettings: () => void;
  pendingQuestion?: {
    id: number;
    question: string;
  };
  onQuestionAccepted?: (id: number) => void;
}) {
  const historyRequest = useRequestGuard();
  const analysisRequest = useRequestGuard();
  const [question, setQuestion] = useState("");
  const latestAnswerRef = useRef<HTMLElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const sendingRef = useRef(false);
  const streamRef = useRef<AbortController | null>(null);
  const [draft, setDraft] = useState<{
    question: string;
    kind?: string;
    answer: string;
  } | null>(null);
  const dreamRef = useRef<Promise<void> | null>(null);
  const [dreaming, setDreaming] = useState(false);
  const [dreamError, setDreamError] = useState("");
  const [memoryOpen, setMemoryOpen] = useState(false);
  const [stage, setStage] = useState("");
  const [turns, setTurns] = useState<Turn[]>([]);
  const [running, setRunning] = useState(false);
  const [stopped, setStopped] = useState(false);
  const [error, setError] = useState("");
  const [history, setHistory] = useState<AnalysisHistoryItem[]>([]);
  const [historyBusy, setHistoryBusy] = useState("");
  const [historyError, setHistoryError] = useState("");
  const [queuedQuestion, setQueuedQuestion] = useState<string | null>(null);
  const handledQuestionId = useRef<number | null>(null);
  const [analysisSettings, setAnalysisSettings] = useState<AnalysisSettings>(
    defaultAnalysisSettings,
  );
  const [settingsSaving, setSettingsSaving] = useState(false);

  const webSearchSupported = model.provider !== "openai-chat";

  useEffect(() => () => streamRef.current?.abort(), []);

  useEffect(() => {
    let mounted = true;
    const refreshSettings = () => {
      void getAnalysisSettings()
        .then((next) => {
          if (mounted) setAnalysisSettings(next);
        })
        .catch(() => {
          // Keep defaults if settings fetch fails before sending.
        });
    };
    refreshSettings();
    window.addEventListener(SERVER_DATA_UPDATED, refreshSettings);
    return () => {
      mounted = false;
      window.removeEventListener(SERVER_DATA_UPDATED, refreshSettings);
    };
  }, []);

  // One consolidation at a time; it changes the data revision, so a new
  // analysis waits for it rather than racing it.
  const dream = () => {
    if (dreamRef.current) return dreamRef.current;
    setDreaming(true);
    setDreamError("");
    const run = dreamMemory()
      .then(() => undefined)
      .catch((nextError) => setDreamError(String(nextError)))
      .finally(() => {
        dreamRef.current = null;
        setDreaming(false);
      });
    dreamRef.current = run;
    return run;
  };

  const loadExplicitQuestion = (text: string) => {
    historyRequest.invalidate();
    setHistoryBusy("");
    setHistoryError("");
    setQuestion(text);
    setTurns([]);
    setDraft(null);
    setStopped(false);
    setError("");
    setQueuedQuestion(null);
  };

  useEffect(() => {
    if (
      !active ||
      !pendingQuestion ||
      handledQuestionId.current === pendingQuestion.id
    )
      return;
    handledQuestionId.current = pendingQuestion.id;
    if (running) {
      setQueuedQuestion(pendingQuestion.question);
      onQuestionAccepted?.(pendingQuestion.id);
      return;
    }
    loadExplicitQuestion(pendingQuestion.question);
    onQuestionAccepted?.(pendingQuestion.id);
  }, [active, running, pendingQuestion, onQuestionAccepted, historyRequest]);

  useEffect(() => {
    let mounted = true;
    const refreshHistory = () =>
      getAnalysisHistory()
        .then((items) => {
          if (mounted) {
            setHistory(items);
            setHistoryError("");
          }
        })
        .catch((nextError) => {
          if (mounted) setHistoryError(String(nextError));
        });
    void refreshHistory();
    window.addEventListener(SERVER_DATA_UPDATED, refreshHistory);
    return () => {
      mounted = false;
      window.removeEventListener(SERVER_DATA_UPDATED, refreshHistory);
    };
  }, []);

  useEffect(() => {
    if (active && (turns.length || draft))
      latestAnswerRef.current?.scrollIntoView?.({
        behavior: "smooth",
        block: "start",
      });
  }, [turns.length, Boolean(draft), active]);

  const send = async () => {
    if (
      !question.trim() ||
      sendingRef.current ||
      historyBusy ||
      !model.hasApiKey
    )
      return;
    sendingRef.current = true;
    const isCurrent = analysisRequest.begin();
    const message = question.trim();
    setRunning(true);
    setStopped(false);
    setError("");
    setDraft({ question: message, answer: "" });
    const controller = new AbortController();
    streamRef.current = controller;
    try {
      if (dreamRef.current) {
        setStage("正在整理长期记忆…");
        await dreamRef.current;
        if (!isCurrent()) return;
      }
      setStage("正在准备资料…");
      // Read the account's saved defaults for every turn, including after a
      // settings change on this device or another device.
      const settings = await getAnalysisSettings();
      if (!isCurrent()) return;
      setAnalysisSettings(settings);
      const request = buildRequest(settings, model, message, turns);
      // Keep the server's immutable context check without an extra UI step.
      const preview = await previewAnalysis(request);
      if (!isCurrent()) return;
      const result = await runAnalysis(
        {
          ...request,
          previewRevision: preview.contextRevision,
        },
        (update) => {
          if (!isCurrent()) return;
          if (update.type === "draft")
            setDraft({
              question: message,
              kind: update.proposal,
              answer: update.answer,
            });
          else setStage(update.message);
        },
        controller.signal,
      );
      if (!isCurrent()) return;
      setTurns((current) => [...current, { question: message, result }]);
      setQuestion("");
      setDraft(null);
      void getAnalysisHistory()
        .then((items) => {
          if (isCurrent()) {
            setHistory(items);
            setHistoryError("");
          }
        })
        .catch((nextError) => {
          if (isCurrent()) setHistoryError(String(nextError));
        });
      void getMemory()
        .then((overview) => {
          if (overview.dreamDue) void dream();
        })
        .catch((nextError) => setDreamError(String(nextError)));
    } catch (nextError) {
      if (isCurrent()) setError(String(nextError));
    } finally {
      // A stopped request may settle after another send has already started.
      if (streamRef.current === controller) {
        sendingRef.current = false;
        streamRef.current = null;
        if (isCurrent()) setRunning(false);
      }
    }
  };

  const stop = () => {
    const controller = streamRef.current;
    if (!controller) return;
    analysisRequest.invalidate();
    streamRef.current = null;
    sendingRef.current = false;
    controller.abort();
    setRunning(false);
    setStopped(true);
    setError("");
  };

  const openStoredAnalysis = async (id: string) => {
    if (sendingRef.current) return false;
    const isCurrent = historyRequest.begin();
    setHistoryBusy(id);
    setHistoryError("");
    try {
      const item = await getAnalysis(id);
      if (!isCurrent()) return false;
      setTurns([{ question: item.question, result: item }]);
      setDraft(null);
      setStopped(false);
      setError("");
    } catch (nextError) {
      if (isCurrent()) setHistoryError(String(nextError));
      return false;
    } finally {
      if (isCurrent()) setHistoryBusy("");
    }
    return isCurrent();
  };

  const newConversation = () => {
    if (sendingRef.current) return;
    historyRequest.invalidate();
    setHistoryBusy("");
    setTurns([]);
    setDraft(null);
    setStopped(false);
    setQuestion("");
    setError("");
    inputRef.current?.focus();
  };

  const [previewOpen, setPreviewOpen] = useState(false);
  const [preview, setPreview] = useState<AnalysisPreview>();
  const [previewError, setPreviewError] = useState("");
  const [previewBusy, setPreviewBusy] = useState(false);
  const [previewQuestionCount, setPreviewQuestionCount] = useState(0);
  const previewRequest = useRequestGuard();

  const loadPreviewFor = async (settings: AnalysisSettings) => {
    const current = previewRequest.begin();
    setPreviewBusy(true);
    setPreviewError("");
    try {
      const request = buildRequest(
        settings,
        model,
        question.trim() || "查看将使用的资料",
        turns,
      );
      setPreviewQuestionCount(
        request.previousTurn
          ? 1 + (request.previousTurn.earlierQuestions?.length ?? 0)
          : 0,
      );
      const next = await previewAnalysis(request);
      if (current()) setPreview(next);
    } catch (nextError) {
      if (current()) setPreviewError(String(nextError));
    } finally {
      if (current()) setPreviewBusy(false);
    }
  };

  const inspectContext = async () => {
    setPreviewOpen(true);
    setPreview(undefined);
    const current = previewRequest.begin();
    setPreviewBusy(true);
    setPreviewError("");
    try {
      const settings = await getAnalysisSettings();
      if (!current()) return;
      setAnalysisSettings(settings);
      await loadPreviewFor(settings);
    } catch (nextError) {
      if (current()) {
        setPreviewError(String(nextError));
        setPreviewBusy(false);
      }
    }
  };

  const updateAnalysisSettings = async (optimistic: AnalysisSettings) => {
    if (settingsSaving || running) return;
    const previous = analysisSettings;
    setAnalysisSettings(optimistic);
    setSettingsSaving(true);
    setPreviewError("");
    try {
      const saved = await saveAnalysisSettings(optimistic);
      setAnalysisSettings(saved);
      if (previewOpen) {
        await loadPreviewFor(saved);
      }
    } catch (nextError) {
      setAnalysisSettings(previous);
      if (previewOpen) setPreviewError(String(nextError));
      else setError(String(nextError));
    } finally {
      setSettingsSaving(false);
    }
  };

  const toggleWebSearch = (value: boolean) =>
    updateAnalysisSettings({ ...analysisSettings, webSearch: value });

  const toggleContextItem = (key: keyof ContextSelection, value: boolean) =>
    updateAnalysisSettings({
      ...analysisSettings,
      contextSelection: {
        ...analysisSettings.contextSelection,
        [key]: value,
      },
    });

  useEffect(() => {
    const input = inputRef.current;
    if (input) {
      input.style.height = "auto";
      input.style.height = `${Math.min(192, Math.max(72, input.scrollHeight))}px`;
    }
  }, [question]);
  const ageGroup = (date: string) => {
    const now = new Date(),
      day = new Date(date);
    const start = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const week = new Date(start);
    week.setDate(week.getDate() - ((week.getDay() + 6) % 7));
    return day >= start ? "今天" : day >= week ? "本周" : "更早";
  };
  const relativeTime = (date: string) => {
    const minutes = Math.max(
      0,
      Math.floor((Date.now() - new Date(date).getTime()) / 60000),
    );
    return minutes < 1
      ? "刚刚"
      : minutes < 60
        ? `${minutes} 分钟前`
        : minutes < 1440
          ? `${Math.floor(minutes / 60)} 小时前`
          : `${Math.floor(minutes / 1440)} 天前`;
  };
  const [historyOpen, setHistoryOpen] = useState(false);
  const includedCount = contextToggleItems.filter(
    (item) => analysisSettings.contextSelection[item.key],
  ).length;
  return (
    <div className="page conversation-page">
      <PageHeader
        title="问答"
        action={
          <>
            <button
              className="secondary history-toggle"
              aria-expanded={historyOpen}
              onClick={() => setHistoryOpen((v) => !v)}
            >
              {historyOpen ? "收起历史" : "历史对话"}
            </button>
            <button className="secondary" onClick={() => setMemoryOpen(true)}>
              {dreaming && <LoaderCircle size={14} className="spin" />}
              长期记忆
            </button>
            <button
              className="secondary"
              onClick={newConversation}
              disabled={running}
            >
              新对话
            </button>
          </>
        }
      />
      <div className="chat-workspace">
        <section
          className={"chat-history" + (historyOpen ? " is-open" : "")}
          aria-label="历史对话"
        >
          <h2>历史对话</h2>
          {history.length === 0 && !historyError && (
            <p className="empty">暂无历史对话</p>
          )}
          <div className="chat-history-list">
            {history.map((item, index) => (
              <div key={item.id} className="chat-history-item">
                {(index === 0 ||
                  ageGroup(history[index - 1].createdAt) !==
                    ageGroup(item.createdAt)) && (
                  <h3>{ageGroup(item.createdAt)}</h3>
                )}
                {
                  <button
                    key={item.id}
                    className={
                      turns.some((turn) => turn.result.id === item.id)
                        ? "active"
                        : ""
                    }
                    onClick={() => {
                      setHistoryOpen(false);
                      void openStoredAnalysis(item.id);
                    }}
                    disabled={running || Boolean(historyBusy)}
                  >
                    <strong>{item.question}</strong>
                    <time
                      dateTime={item.createdAt}
                      title={new Date(item.createdAt).toLocaleString("zh-CN")}
                    >
                      {relativeTime(item.createdAt)}
                    </time>
                    {historyBusy === item.id ? (
                      <LoaderCircle size={14} className="spin" />
                    ) : (
                      <ChevronRight size={14} />
                    )}
                  </button>
                }
              </div>
            ))}
          </div>
        </section>
        <div className="chat-main">
          {turns.length === 0 && !draft && !running && (
            <section className="conversation-welcome" aria-label="开始问答">
              <div className="conversation-starters">
                {[
                  "结合我当前持仓和现金，分析组合最主要的风险集中点。",
                  "帮我复盘最近一笔交易，检查当时的判断依据和潜在盲点。",
                  "结合我的研究资料，帮我梳理当前最值得优先跟踪的公司。",
                ].map((prompt) => (
                  <button
                    key={prompt}
                    aria-label={prompt}
                    type="button"
                    disabled={Boolean(historyBusy)}
                    onClick={() => {
                      setQuestion(prompt);
                      inputRef.current?.focus();
                    }}
                  >
                    {prompt}
                  </button>
                ))}
              </div>
            </section>
          )}
          <div className="conversation-turns" aria-label="当前对话">
            {turns.map((turn, index) => (
              <article
                className="conversation-turn"
                key={turn.result.id}
                ref={index === turns.length - 1 ? latestAnswerRef : undefined}
              >
                <div className="user-question">
                  <span>你</span>
                  <p>{turn.question}</p>
                </div>
                <div className="assistant-answer">
                  <div className="answer-author">
                    mario ·{" "}
                    <DataStamp
                      date={new Date(turn.result.createdAt).toLocaleString(
                        "zh-CN",
                      )}
                    />
                    {(() => {
                      const count = reportSources(turn.result.workflowTrace)
                        .sources.length;
                      return count > 0 ? (
                        <span> · 引用 {count} 条来源</span>
                      ) : null;
                    })()}
                  </div>
                  <ModelReport
                    answer={turn.result.answer}
                    trace={turn.result.workflowTrace}
                  />
                </div>
              </article>
            ))}
            {draft && (
              <article className="conversation-turn" ref={latestAnswerRef}>
                <div className="user-question">
                  <span>你</span>
                  <p>{draft.question}</p>
                </div>
                <div className="assistant-answer" aria-busy={running}>
                  <span className="answer-author">
                    mario ·{" "}
                    {running
                      ? "正在生成，内容尚待校验"
                      : stopped
                        ? "已停止生成"
                        : "回复未完成"}
                    <Badge tone="warning">未校验</Badge>
                  </span>
                  {running && <GenerationSteps stage={stage} />}
                  {draft.answer ? (
                    <section
                      className="stream-draft"
                      aria-label={
                        draft.kind === "DIRECT" ? "回复草稿" : "策略汇总草稿"
                      }
                    >
                      <h3>
                        {draft.kind === "DIRECT"
                          ? "回复草稿"
                          : "策略汇总 · 草稿"}
                      </h3>
                      <ModelReport answer={draft.answer} />
                    </section>
                  ) : (
                    <ModelReport
                      answer={
                        running
                          ? stage
                          : stopped
                            ? "已停止生成。"
                            : "未收到完整回复，请重试。"
                      }
                    />
                  )}
                </div>
              </article>
            )}
          </div>
          {!model.hasApiKey && (
            <Callout
              tone="info"
              action={
                <button className="text-button" onClick={onOpenSettings}>
                  前往设置
                </button>
              }
            >
              连接模型后即可开始问答。
            </Callout>
          )}
          {queuedQuestion && (
            <Callout
              tone="info"
              onClose={() => setQueuedQuestion(null)}
              action={
                <button
                  type="button"
                  className="text-button"
                  disabled={running}
                  onClick={() => {
                    loadExplicitQuestion(queuedQuestion);
                    inputRef.current?.focus();
                  }}
                >
                  {running ? "等待当前生成完成…" : "载入新问题"}
                </button>
              }
            >
              已暂存来自其他页面的研究问题。
            </Callout>
          )}
          {error && <Callout tone="danger">{error}</Callout>}
          {historyError && <Callout tone="danger">{historyError}</Callout>}
          <form
            className="panel conversation-composer"
            onSubmit={(event) => {
              event.preventDefault();
              void send();
            }}
          >
            <label className="question-box">
              <span className="sr-only">输入问题</span>
              <textarea
                disabled={running || Boolean(historyBusy)}
                ref={inputRef}
                value={question}
                placeholder={turns.length ? "继续追问…" : "输入你的问题…"}
                onChange={(event) => setQuestion(event.target.value)}
                onKeyDown={(event) => {
                  if (
                    event.key === "Enter" &&
                    (event.metaKey || event.ctrlKey) &&
                    !event.nativeEvent.isComposing
                  ) {
                    event.preventDefault();
                    void send();
                  }
                }}
              />
            </label>
            <div className="composer-footer">
              <div className="composer-controls" data-presentation="true">
                <button
                  type="button"
                  role="switch"
                  aria-checked={
                    webSearchSupported && analysisSettings.webSearch
                  }
                  className={`composer-chip ${webSearchSupported && analysisSettings.webSearch ? "active" : ""}`}
                  disabled={
                    !webSearchSupported ||
                    running ||
                    Boolean(historyBusy) ||
                    settingsSaving
                  }
                  title={
                    webSearchSupported
                      ? "允许模型联网核查最新公开信息"
                      : "当前模型接口不支持联网搜索"
                  }
                  onClick={() =>
                    void toggleWebSearch(!analysisSettings.webSearch)
                  }
                >
                  <Globe size={14} aria-hidden="true" />
                  联网搜索
                </button>
                <button
                  type="button"
                  className="composer-chip"
                  disabled={running || Boolean(historyBusy)}
                  onClick={() => void inspectContext()}
                >
                  <SlidersHorizontal size={14} aria-hidden="true" />
                  上下文资料（{includedCount}/{contextToggleItems.length}）
                </button>
              </div>
              <span className="composer-status" role="status">
                {running
                  ? stage
                  : stopped
                    ? "已停止，已显示的内容尚未完成"
                    : "Ctrl / ⌘ + Enter 发送"}
              </span>
              {running ? (
                <button
                  type="button"
                  className="secondary danger-text"
                  onClick={stop}
                >
                  <Square size={17} />
                  停止生成
                </button>
              ) : (
                <button
                  type="submit"
                  className="primary"
                  disabled={
                    Boolean(historyBusy) || !question.trim() || !model.hasApiKey
                  }
                >
                  <Send size={17} />
                  发送
                </button>
              )}
            </div>
          </form>
        </div>
      </div>
      {memoryOpen && (
        <Drawer title="长期记忆" onClose={() => setMemoryOpen(false)}>
          <MemoryPanel
            dreaming={dreaming}
            dreamError={dreamError}
            onDream={() => void dream()}
            onOpenAnalysis={(id) => {
              if (sendingRef.current) return;
              setMemoryOpen(false);
              void openStoredAnalysis(id);
            }}
          />
        </Drawer>
      )}
      {previewOpen && (
        <Drawer
          title="上下文资料与发送预览"
          size="wide"
          busy={previewBusy || settingsSaving}
          onClose={() => {
            previewRequest.invalidate();
            setPreviewOpen(false);
          }}
        >
          <section
            className="context-scope-bar"
            aria-label="发送给模型的资料范围"
            data-presentation="true"
          >
            {contextToggleItems.map((item) => (
              <label key={item.key} className="context-scope-toggle">
                <input
                  type="checkbox"
                  checked={analysisSettings.contextSelection[item.key]}
                  disabled={settingsSaving || running}
                  onChange={(event) =>
                    void toggleContextItem(item.key, event.target.checked)
                  }
                />
                <span>{item.label}</span>
              </label>
            ))}
            <label
              className="context-scope-toggle"
              title={
                webSearchSupported ? undefined : "当前模型接口不支持联网搜索"
              }
            >
              <input
                type="checkbox"
                checked={webSearchSupported && analysisSettings.webSearch}
                disabled={!webSearchSupported || settingsSaving || running}
                onChange={(event) => void toggleWebSearch(event.target.checked)}
              />
              <span>联网搜索</span>
            </label>
          </section>
          {previewBusy && <p role="status">正在读取预览…</p>}
          {previewError && <Callout tone="danger">{previewError}</Callout>}
          {preview && (
            <ContextPreview
              preview={preview}
              previousQuestionCount={previewQuestionCount}
            />
          )}
        </Drawer>
      )}
    </div>
  );
}
