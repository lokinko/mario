import { useEffect, useRef, useState } from "react";
import { serviceUrl } from "../../lib/service";
import { requestJson } from "../../lib/transport";
import { openAiSignIn } from "./openaiLogin";

export function OpenAiSignIn({
  link = false,
  disabled = false,
}: {
  link?: boolean;
  disabled?: boolean;
}) {
  const [enabled, setEnabled] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const controller = useRef<AbortController | null>(null);
  const alive = useRef(false);
  useEffect(() => {
    alive.current = true;
    const config = new AbortController();
    if (
      new URL(serviceUrl(), window.location.href).origin ===
      window.location.origin
    )
      void requestJson<{ enabled: boolean }>(
        `${serviceUrl()}/auth/openai/config`,
        { signal: config.signal },
      )
        .then((value) => {
          if (!config.signal.aborted) setEnabled(value.enabled === true);
        })
        .catch(() => {});
    return () => {
      alive.current = false;
      config.abort();
      controller.current?.abort();
    };
  }, []);
  if (!enabled) return null;
  return (
    <div className="form-grid single-column">
      <button
        type="button"
        className="secondary"
        data-oai-siwc-button=""
        disabled={disabled || busy}
        onClick={async () => {
          const current = new AbortController();
          controller.current = current;
          setBusy(true);
          setMessage("");
          try {
            const url = await openAiSignIn(link, current.signal);
            if (!alive.current || current.signal.aborted) return;
            window.location.assign(url);
          } catch (error) {
            if (alive.current && !current.signal.aborted)
              setMessage(
                error instanceof Error
                  ? error.message
                  : "ChatGPT 登录失败，请重试。",
              );
          } finally {
            if (alive.current) setBusy(false);
          }
        }}
      >
        {busy
          ? "等待 ChatGPT 授权…"
          : link
            ? "关联 ChatGPT 登录"
            : "Continue with ChatGPT"}
      </button>
      <small>仅用于 Mario 账号登录与关联；模型订阅授权单独开通。</small>
      {message && <p role="status">{message}</p>}
    </div>
  );
}
