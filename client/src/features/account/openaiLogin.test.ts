// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { completeOpenAiSignIn, openAiSignIn } from "./openaiLogin";
import { requestJson } from "../../lib/transport";
import { setWebToken } from "../../lib/webSession";
vi.mock("../../lib/transport", () => ({ requestJson: vi.fn() }));
vi.mock("../../lib/service", () => ({ serviceUrl: () => "/api" }));
afterEach(() => {
  vi.resetAllMocks();
  localStorage.clear();
  sessionStorage.clear();
});
it("requests only an identity authorization URL without sending a Mario token for normal login", async () => {
  vi.mocked(requestJson).mockResolvedValue({
    url: "https://auth.openai.com/api/accounts/authorize?state=public",
  });
  expect(await openAiSignIn(false, new AbortController().signal)).toContain(
    "https://auth.openai.com/",
  );
  expect(requestJson).toHaveBeenCalledWith(
    "/api/auth/openai/start",
    expect.objectContaining({ method: "POST", body: '{"link":false}' }),
  );
  expect(vi.mocked(requestJson).mock.calls[0][1]?.headers).not.toHaveProperty(
    "Authorization",
  );
});
it("requires the current Mario session for explicit linking", async () => {
  await expect(
    openAiSignIn(true, new AbortController().signal),
  ).rejects.toThrow("请先登录");
  setWebToken("existing");
  vi.mocked(requestJson).mockResolvedValue({
    url: "https://auth.openai.com/authorize",
  });
  await openAiSignIn(true, new AbortController().signal);
  expect(requestJson).toHaveBeenCalledWith(
    expect.any(String),
    expect.objectContaining({
      headers: expect.objectContaining({ Authorization: "Bearer existing" }),
    }),
  );
});
it("rejects unsafe redirects, account switches and cancelled late responses", async () => {
  vi.mocked(requestJson).mockResolvedValue({ url: "https://evil.test/" });
  await expect(
    openAiSignIn(false, new AbortController().signal),
  ).rejects.toThrow("不属于 OpenAI");
  let finish!: (v: { url: string }) => void;
  vi.mocked(requestJson).mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  const pending = openAiSignIn(false, new AbortController().signal);
  setWebToken("changed");
  finish({ url: "https://auth.openai.com/authorize" });
  await expect(pending).rejects.toThrow("账号已切换");
  const controller = new AbortController();
  vi.mocked(requestJson).mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  const cancelled = openAiSignIn(false, controller.signal);
  controller.abort();
  finish({ url: "https://auth.openai.com/authorize" });
  await expect(cancelled).rejects.toThrow();
});
it("completes through a same-origin POST and accepts only Mario's session shape", async () => {
  const session = {
    token: "a".repeat(64),
    userId: "user",
    email: "user@example.test",
    expiresAt: Date.now() / 1000 + 3600,
  };
  const prepare = async () => {
    vi.mocked(requestJson).mockResolvedValue({
      url: "https://auth.openai.com/authorize",
    });
    await openAiSignIn(false, new AbortController().signal);
  };
  await prepare();
  vi.mocked(requestJson).mockResolvedValue({ session, linked: false });
  expect((await completeOpenAiSignIn()).session).toEqual(session);
  expect(requestJson).toHaveBeenCalledWith("/api/auth/openai/complete", {
    method: "POST",
  });
  await prepare();
  vi.mocked(requestJson).mockResolvedValue({
    session: { ...session, token: "upstream-token" },
    linked: false,
  });
  await expect(completeOpenAiSignIn()).rejects.toThrow("返回结果无效");
  await prepare();
  vi.mocked(requestJson).mockResolvedValue({ session: null, linked: true });
  expect((await completeOpenAiSignIn()).linked).toBe(true);
});
it("refuses stale or missing login context before consuming the HttpOnly handoff", async () => {
  await expect(completeOpenAiSignIn()).rejects.toThrow("上下文已过期");
  expect(requestJson).not.toHaveBeenCalled();
  vi.mocked(requestJson).mockResolvedValue({
    url: "https://auth.openai.com/authorize",
  });
  await openAiSignIn(false, new AbortController().signal);
  vi.mocked(requestJson).mockClear();
  setWebToken("other-account");
  await expect(completeOpenAiSignIn()).rejects.toThrow("账号已切换");
  expect(requestJson).not.toHaveBeenCalled();
});
