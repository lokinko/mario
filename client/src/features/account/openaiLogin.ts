import { serviceUrl } from "../../lib/service";
import { requestJson } from "../../lib/transport";
import { webToken } from "../../lib/webSession";

export interface MarioLoginSession {
  token: string;
  userId: string;
  email: string;
  expiresAt: number;
}
export interface OpenAiLoginResult {
  session: MarioLoginSession | null;
  linked: boolean;
}
function sameOriginApi() {
  const base = serviceUrl();
  if (new URL(base, window.location.href).origin !== window.location.origin)
    throw new Error("ChatGPT 网站登录需要从同源部署的 Mario 页面发起。");
  return base;
}
/** Creates the browser-bound transaction; only the public OAuth URL is returned. */
export async function openAiSignIn(
  link: boolean,
  signal: AbortSignal,
): Promise<string> {
  const base = sameOriginApi();
  const startingToken = webToken();
  if (link && !startingToken)
    throw new Error("请先登录 Mario，再关联 ChatGPT。");
  const { url } = await requestJson<{ url: string }>(
    `${base}/auth/openai/start`,
    {
      method: "POST",
      signal,
      headers: {
        "Content-Type": "application/json",
        ...(link ? { Authorization: `Bearer ${startingToken}` } : {}),
      },
      body: JSON.stringify({ link }),
    },
  );
  signal.throwIfAborted();
  if (webToken() !== startingToken || serviceUrl() !== base)
    throw new Error("Mario 账号已切换，请重新发起 ChatGPT 登录。");
  const target = new URL(url);
  if (target.origin !== "https://auth.openai.com")
    throw new Error("授权地址不属于 OpenAI。");
  sessionStorage.setItem(
    `mario.openai-login:${base}`,
    JSON.stringify({ token: startingToken, createdAt: Date.now() }),
  );
  return target.href;
}
/** The HttpOnly, browser-bound handoff is consumed once; never retried. */
export async function completeOpenAiSignIn(): Promise<OpenAiLoginResult> {
  const base = sameOriginApi();
  const key = `mario.openai-login:${base}`;
  let context;
  try {
    context = JSON.parse(sessionStorage.getItem(key) || "null");
  } catch {
    context = null;
  }
  sessionStorage.removeItem(key);
  if (
    !context ||
    context.token !== webToken() ||
    !Number.isFinite(context.createdAt) ||
    Date.now() - context.createdAt > 600000
  )
    throw new Error("Mario 账号已切换或登录上下文已过期，请重新登录。");
  const result = await requestJson<OpenAiLoginResult>(
    `${base}/auth/openai/complete`,
    { method: "POST" },
  );
  if (context.token !== webToken() || base !== serviceUrl())
    throw new Error("Mario 账号已切换，请重新登录。");
  if (result.linked === true && result.session === null) return result;
  if (result.linked === false && validSession(result.session)) return result;
  throw new Error("登录返回结果无效，请重新登录。");
}
function validSession(value: unknown): value is MarioLoginSession {
  const session = value as MarioLoginSession | null;
  return (
    !!session &&
    typeof session.token === "string" &&
    /^[a-f0-9]{64}$/.test(session.token) &&
    typeof session.userId === "string" &&
    session.userId.length > 0 &&
    session.userId.length <= 100 &&
    typeof session.email === "string" &&
    session.email.length <= 254 &&
    Number.isFinite(session.expiresAt) &&
    session.expiresAt * 1000 > Date.now()
  );
}
