import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { Badge } from "../../components/Badge";
import { DataStamp } from "../../components/DataStamp";
import { GenerationSteps } from "./GenerationSteps";
import { reportSources } from "./ReportSources";
import { PageHeader } from "../../components/PageHeader";
import { SERVER_DATA_UPDATED } from "../../lib/syncEvents";
import { useEffect, useRef, useState } from "react";
import { ChevronRight, LoaderCircle, Send, Square } from "lucide-react";
import {
  getAnalysis,
  getAnalysisHistory,
  previewAnalysis,
  runAnalysis,
} from "../../api/analysis";
import { getAnalysisSettings } from "../../api/settings";
import { dreamMemory, getMemory } from "../../api/memory";
import { MemoryPanel } from "./MemoryPanel";
import type {
  AnalysisRequest,
  AnalysisSettings,
  AnalysisPreview,
  AnalysisResult,
  AnalysisHistoryItem,
  ModelConfig,
  StoredAnalysis,
} from "../../types/ai";
import { ModelReport } from "./ModelReport";
import { previousTurn } from "./previousTurn";
import { useRequestGuard } from "../../lib/useRequestGuard";

type Turn = { question: string; result: AnalysisResult | StoredAnalysis };

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
    proposals: Record<string, string>;
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

  useEffect(() => () => streamRef.current?.abort(), []);

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
    setDraft({ question: message, proposals: {} });
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
            setDraft((current) => ({
              question: message,
              proposals: {
                ...current?.proposals,
                [update.proposal]: update.answer,
              },
            }));
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
  const inspectContext = async () => {
    const current = previewRequest.begin();
    setPreviewOpen(true);
    setPreviewBusy(true);
    setPreview(undefined);
    setPreviewError("");
    try {
      const settings = await getAnalysisSettings();
      if (!current()) return;
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
    } catch (error) {
      if (current()) setPreviewError(String(error));
    } finally {
      if (current()) setPreviewBusy(false);
    }
  };
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
                  "如何核查我关注公司的分红持续性？",
                  "请结合我的真实交易讨论最近一次红利投资决策。",
                  "对我认可的红利公司，还需要等待哪些条件再考虑买入？",
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
                    <span>
                      {" "}
                      · 引用{" "}
                      {
                        reportSources(turn.result.workflowTrace).sources.length
                      }{" "}
                      条来源
                    </span>
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
                  {Object.keys(draft.proposals).length ? (
                    Object.entries(draft.proposals)
                      .sort(([a], [b]) => a.localeCompare(b))
                      .map(([proposal, answer]) => (
                        <section
                          key={proposal}
                          className="proposal-draft"
                          aria-label={`方案 ${proposal} 草稿`}
                        >
                          <h3>方案 {proposal} · 草稿</h3>
                          <ModelReport answer={answer} />
                        </section>
                      ))
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
              已暂存来自其他页面的研究问题，避免中断或覆盖当前对话。
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
              <span role="status">
                {running
                  ? stage
                  : stopped
                    ? "已停止，已显示的内容尚未完成"
                    : "Ctrl / ⌘ + Enter 发送"}
              </span>
              <button
                type="button"
                className="text-button"
                disabled={running || Boolean(historyBusy)}
                onClick={() => void inspectContext()}
              >
                查看将发送的数据
              </button>
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
          title="将发送的数据"
          busy={previewBusy}
          onClose={() => {
            previewRequest.invalidate();
            setPreviewOpen(false);
          }}
        >
          {previewBusy && <p role="status">正在读取预览…</p>}
          {previewError && <Callout tone="danger">{previewError}</Callout>}
          {preview && (
            <>
              <DataStamp date={preview.generatedAt} source={preview.model} />
              <p className="section-intro">
                仅使用问答预设中选择的资料。以下为当前预览；发送前会重新核对并冻结输入。
              </p>
              <p>
                {previewQuestionCount
                  ? `承接 ${previewQuestionCount} 条用户问题，并附上轮回答摘要与结构化追问（如有）。`
                  : "新对话，不承接历史问题或上轮追问。"}
              </p>
              <ul>
                {preview.groups.map((group) => (
                  <li key={group.key}>
                    {group.label} ·{" "}
                    {group.included ? `${group.recordCount} 条` : "未选择"}
                    <p>{group.description}</p>
                  </li>
                ))}
              </ul>
              <h3>实际资料正文</h3>
              <pre className="context-preview">
                {JSON.stringify(preview.payload, null, 2)}
              </pre>
              <h3>仅本地保留</h3>
              <ul>
                {preview.localOnly.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </>
          )}
        </Drawer>
      )}
    </div>
  );
}
