import { SERVER_DATA_UPDATED } from "../../lib/syncEvents";
import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  ChevronRight,
  LoaderCircle,
  Send,
  Square,
} from "lucide-react";
import {
  getAnalysis,
  getAnalysisHistory,
  previewAnalysis,
  runAnalysis,
} from "../../api/research";
import { getAnalysisSettings } from "../../api/settings";
import type {
  AnalysisRequest,
  AnalysisResult,
  AnalysisHistoryItem,
  ModelConfig,
  StoredAnalysis,
} from "../../types";
import { View } from "../../app/navigation";
import { ModelReport } from "./ModelReport";
import { useRequestGuard } from "../../lib/useRequestGuard";

type Turn = { question: string; result: AnalysisResult | StoredAnalysis };

export function Advisor({
  model,
  active = true,
  navigate,
}: {
  model: ModelConfig;
  active?: boolean;
  navigate: (v: View) => void;
}) {
  const historyRequest = useRequestGuard();
  const analysisRequest = useRequestGuard();
  const [question, setQuestion] = useState("");
  const [dailyAssetRange, setDailyAssetRange] = useState<{
    from: string;
    to: string;
  }>();
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
    if (!active || running) return;
    const queued =
      window.sessionStorage.getItem("mario.advisorQuestion") ??
      window.sessionStorage.getItem("compass.advisorQuestion");
    if (!queued) return;
    const range = window.sessionStorage.getItem("mario.dailyAssetRange");
    window.sessionStorage.removeItem("mario.dailyAssetRange");
    try {
      const parsed = range ? JSON.parse(range) : undefined;
      setDailyAssetRange(
        parsed &&
          typeof parsed.from === "string" &&
          typeof parsed.to === "string"
          ? parsed
          : undefined,
      );
    } catch {
      setDailyAssetRange(undefined);
    }
    setQuestion(queued);
    setTurns([]);
    setDraft(null);
    setStopped(false);
    setError("");
    window.sessionStorage.removeItem("mario.advisorQuestion");
    window.sessionStorage.removeItem("compass.advisorQuestion");
  }, [active, running]);

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
        dailyAssetRange,
        userMessage: message,
        question: previous
          ? `以下是上一轮问答，仅用于理解追问，模型回答不是已核实事实。\n上次问题：${previous.question}\n上次回答摘要：${previous.result.answer.slice(0, 6000)}\n\n本次问题：${message}`
          : message,
        excludedMemoryIds: [],
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
      setDailyAssetRange(undefined);
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
    setDailyAssetRange(undefined);
    setError("");
    inputRef.current?.focus();
  };

  return (
    <div className="page narrow conversation-page">
      <header className="conversation-heading chat-heading">
        <h1>问答</h1>
        <button
          className="secondary"
          onClick={newConversation}
          disabled={running}
        >
          新对话
        </button>
      </header>
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
              <span className="answer-author">mario</span>
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
              </span>
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
      <form
        className="panel conversation-composer"
        onSubmit={(event) => {
          event.preventDefault();
          void send();
        }}
      >
        <label className="question-box">
          <span className="visually-hidden">输入问题</span>
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
            {!model.hasApiKey && (
              <button
                type="button"
                className="text-button"
                onClick={() => navigate("settings")}
              >
                前往设置连接模型
              </button>
            )}
          </span>
          {running ? (
            <button type="button" className="secondary" onClick={stop}>
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
      {error && (
        <div className="error-box" role="alert">
          <AlertTriangle size={18} />
          {error}
        </div>
      )}
      {historyError && (
        <div className="error-box" role="alert">
          <AlertTriangle size={18} />
          {historyError}
        </div>
      )}
      <section className="chat-history" aria-label="历史对话">
        <h2>历史对话</h2>
        {history.length === 0 && !historyError && (
          <p className="empty">暂无历史对话</p>
        )}
        <div className="chat-history-list">
          {history.map((item) => (
            <button
              key={item.id}
              className={
                turns.some((turn) => turn.result.id === item.id) ? "active" : ""
              }
              onClick={() => void openStoredAnalysis(item.id)}
              disabled={running || Boolean(historyBusy)}
            >
              <strong>{item.question}</strong>
              <time dateTime={item.createdAt}>
                {new Date(item.createdAt).toLocaleString("zh-CN")}
              </time>
              {historyBusy === item.id ? (
                <LoaderCircle size={14} className="spin" />
              ) : (
                <ChevronRight size={14} />
              )}
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}
