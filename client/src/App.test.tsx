// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import App from "./App";
import * as api from "./api";

vi.mock("./api", async (original) => ({
  ...(await original<typeof import("./api")>()),
  ensureDailyAssets: vi.fn().mockResolvedValue({
    timezone: "UTC",
    today: new Date().toISOString().slice(0, 10),
    records: [],
    nextBefore: null,
  }),
  getSnapshot: vi.fn(),
  getModelConfig: vi.fn(),
  getAnalysisHistory: vi.fn().mockResolvedValue([]),
}));
vi.mock("./reminders", () => ({
  checkAndSendReviewReminder: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("./features/account/CloudSync", () => ({
  CloudSync: ({ onRestore }: { onRestore: () => Promise<void> }) => (
    <button onClick={() => void onRestore()}>测试恢复数据</button>
  ),
}));
beforeEach(() => {
  vi.clearAllMocks();
  window.location.hash = "cloud";
});
afterEach(cleanup);

it("keeps local application accessible when optional model configuration fails", async () => {
  vi.mocked(api.getSnapshot).mockResolvedValue(
    {} as Awaited<ReturnType<typeof api.getSnapshot>>,
  );
  vi.mocked(api.getModelConfig).mockRejectedValue(
    new Error("keychain unavailable"),
  );
  render(<App />);
  expect(
    await screen.findByRole("button", { name: "测试恢复数据" }),
  ).toBeTruthy();
  expect((await screen.findByRole("alert")).textContent).toContain(
    "本地决策与复盘不受影响",
  );
  expect(screen.queryByText("本地服务尚未就绪")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "测试恢复数据" }));
  await waitFor(() => expect(api.getSnapshot).toHaveBeenCalledTimes(2));
});

it("offers retry when required local data fails to load", async () => {
  vi.mocked(api.getSnapshot)
    .mockRejectedValueOnce(new Error("local unavailable"))
    .mockResolvedValue({} as Awaited<ReturnType<typeof api.getSnapshot>>);
  vi.mocked(api.getModelConfig).mockRejectedValue(
    new Error("model unavailable"),
  );
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "重新连接" }));
  expect(
    await screen.findByRole("button", { name: "测试恢复数据" }),
  ).toBeTruthy();
});

it("opens on questions and preserves a draft when returning from supporting pages", async () => {
  window.location.hash = "";
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  vi.mocked(api.getSnapshot).mockResolvedValue(
    {} as Awaited<ReturnType<typeof api.getSnapshot>>,
  );
  vi.mocked(api.getModelConfig).mockResolvedValue({
    provider: "openai-compatible",
    model: "test",
    baseUrl: "",
    hasApiKey: true,
  });
  render(<App />);
  expect(
    await screen.findByRole("heading", { name: "最近有什么投资上的困惑？" }),
  ).toBeTruthy();
  fireEvent.change(screen.getByRole("textbox"), {
    target: { value: "暂存的问题" },
  });
  fireEvent.click(screen.getByRole("button", { name: "账户与同步" }));
  await screen.findByRole("button", { name: "测试恢复数据" });
  expect(screen.queryByRole("textbox")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "问答" }));
  expect(screen.getByDisplayValue("暂存的问题")).toBeTruthy();
});

it("keeps everyday navigation to questions and personal facts", async () => {
  vi.mocked(api.getSnapshot).mockResolvedValue(
    {} as Awaited<ReturnType<typeof api.getSnapshot>>,
  );
  vi.mocked(api.getModelConfig).mockResolvedValue({
    provider: "codex",
    model: "test",
    baseUrl: "codex://local",
    hasApiKey: true,
  });
  render(<App />);
  await screen.findByRole("button", { name: "问答" });
  expect(screen.getByRole("button", { name: "我的情况" })).toBeTruthy();
  for (const name of [
    "决策确认",
    "参考资料",
    "记忆与规则",
    "复盘记录",
    "组合流水",
    "资料与确认",
  ])
    expect(screen.queryByRole("button", { name })).toBeNull();
});
it("responds to hash navigation without leaving the old page on screen", async () => {
  vi.mocked(api.getSnapshot).mockResolvedValue(
    {} as Awaited<ReturnType<typeof api.getSnapshot>>,
  );
  vi.mocked(api.getModelConfig).mockResolvedValue({
    provider: "openai-responses",
    baseUrl: "https://api.openai.com/v1",
    model: "test",
    hasApiKey: false,
  });
  render(<App />);
  await screen.findByRole("button", { name: "测试恢复数据" });
  window.location.hash = "advisor";
  fireEvent(window, new HashChangeEvent("hashchange"));
  await screen.findByRole("textbox", { name: "这次希望解决什么问题？" });
  expect(screen.queryByRole("button", { name: "测试恢复数据" })).toBeNull();
});

it("closes the left drawer with Escape and restores focus without changing the page", async () => {
  vi.mocked(api.getSnapshot).mockResolvedValue(
    {} as Awaited<ReturnType<typeof api.getSnapshot>>,
  );
  vi.mocked(api.getModelConfig).mockResolvedValue({
    provider: "codex",
    model: "test",
    baseUrl: "codex://local",
    hasApiKey: true,
  });
  render(<App />);
  const trigger = await screen.findByRole("button", { name: "打开导航" });
  fireEvent.click(trigger);
  expect(
    document.querySelector("aside")?.classList.contains("mobile-open"),
  ).toBe(true);
  expect(document.querySelector("main")?.hasAttribute("inert")).toBe(true);
  fireEvent.keyDown(window, { key: "Escape" });
  expect(
    document.querySelector("aside")?.classList.contains("mobile-open"),
  ).toBe(false);
  expect(document.activeElement).toBe(trigger);
  expect(document.querySelector("main")?.hasAttribute("inert")).toBe(false);
  expect(screen.getByRole("button", { name: "测试恢复数据" })).toBeTruthy();
});
