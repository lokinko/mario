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
} from "../../api/research";
import { getAnalysisSettings } from "../../api/settings";
import type {
  AnalysisRequest,
  AnalysisPreview,
  AnalysisResult,
  AnalysisHistoryItem,
  ModelConfig,
  StoredAnalysis,
} from "../../types";
import { ModelReport } from "./ModelReport";
import { useRequestGuard } from "../../lib/useRequestGuard";

type Turn = { question: string; result: AnalysisResult | StoredAnalysis };

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
    answer: string;
  } | null>(null);
  const [stage, setStage] = useState("");
  const [turns, setTurns] = useState<Turn[]>([]);
  const [running, setRunning] = useState(false);
  const [stopped, setStopped] = useState(false);
  const [error, setError] = useState("");
  const [history, setHistory] = useState<AnalysisHistoryItem[]>([]);
  const [historyBusy, setHistoryBusy] = useState("");
  const [historyError, setHistoryError] = useState("");

  useEffect(() => () => streamRef.current?.abort(), []);

  useEffect(() => {
    if (!active || running || !pendingQuestion) return;
    setQuestion(pendingQuestion.question);
    setTurns([]);
    setDraft(null);
    setStopped(false);
    setError("");
    onQuestionAccepted?.(pendingQuestion.id);
  }, [active, running, pendingQuestion, onQuestionAccepted]);

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
    const previous = turns.at(-1);
    setRunning(true);
    setStopped(false);
    setError("");
    setDraft({ question: message, answer: "" });
    setStage("正在准备资料…");
    const controller = new AbortController();
    streamRef.current = controller;
    try {
      // Read the account's saved defaults for every turn, including after a
      // settings change on this device or another device.
      const settings = await getAnalysisSettings();
      if (!isCurrent()) return;
      const request: AnalysisRequest = {
        ...settings,
        webSearch: settings.webSearch && model.provider !== "openai-chat",
        userMessage: message,
        question: previous
          ? `以下是上一轮问答，仅用于理解追问，模型回答不是已核实事实。\n上次问题：${previous.question}\n上次回答摘要：${previous.result.answer.slice(0, 6000)}\n\n本次问题：${message}`
          : message,
      };
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
            setDraft({ question: message, answer: update.answer });
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
      setQuestion("");
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
      const next = await previewAnalysis({
        ...settings,
        webSearch: settings.webSearch && model.provider !== "openai-chat",
        question: question.trim() || "查看将使用的资料",
      });
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
              历史对话
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
              <h2>从一个问题开始</h2>
              <p>
                按你的问答预设读取已选持仓、现金、方法论与复盘规则，梳理依据、风险和待核实的信息。
              </p>
              <button
                className="text-button"
                onClick={() => void inspectContext()}
              >
                查看将发送的数据
              </button>
              <div className="conversation-starters">
                {[
                  [
                    "风险",
                    "帮我梳理当前持仓的主要风险",
                    "识别集中度与信息缺口",
                  ],
                  [
                    "配置",
                    "我的现金和投资配置是否合理？",
                    "先核对现金流与风险承受能力",
                  ],
                  [
                    "复盘",
                    "如何复盘最近的一次投资决策？",
                    "回看当时依据与证伪条件",
                  ],
                ].map(([category, prompt, description]) => (
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
                    <Badge>{category}</Badge>
                    <strong>{prompt}</strong>
                    <small>{description}</small>
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
                  <ModelReport
                    answer={
                      draft.answer ||
                      (running
                        ? stage
                        : stopped
                          ? "已停止生成。"
                          : "未收到完整回复，请重试。")
                    }
                  />
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
              <button
                className="text-button composer-context"
                type="button"
                disabled={running}
                onClick={() => void inspectContext()}
              >
                将使用：查看已选资料
              </button>
              <span role="status">
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
          {error && <Callout tone="danger">{error}</Callout>}
          {historyError && <Callout tone="danger">{historyError}</Callout>}
        </div>
      </div>
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
