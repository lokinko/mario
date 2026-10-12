// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import App from "./App";
import * as api from "./api";
import emptySnapshot from "./test/emptySnapshot.json";
import { analysisSettings as defaultAnalysisSettings } from "./test/analysisSettings";
import { WebAccess } from "./features/account/WebAccess";
import * as service from "./lib/service";
import { setWebToken, webToken } from "./lib/webSession";

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
  getReviewReminders: vi.fn().mockResolvedValue(null),
  getDailyAssets: vi.fn().mockResolvedValue({ records: [], nextBefore: null }),
  getPortfolioEvents: vi.fn().mockResolvedValue([]),
}));
vi.mock("./features/account/ServerData", () => ({
  ServerData: ({ onRestore }: { onRestore: () => Promise<void> }) => (
    <button onClick={() => void onRestore()}>测试恢复数据</button>
  ),
}));
beforeEach(() => {
  vi.clearAllMocks();
  window.location.hash = "cloud";
});
afterEach(cleanup);

it.each([true, false])(
  "switches accounts from the settings page even when model configuration is available=%s",
  async (modelAvailable) => {
    const localDev = vi.spyOn(service, "isLocalDev").mockReturnValue(false);
    const securityConfig = vi
      .spyOn(api, "getSecurityPriceConfig")
      .mockResolvedValue({ provider: "twelve-data", hasApiKey: false });
    const analysisSettings = vi
      .spyOn(api, "getAnalysisSettings")
      .mockResolvedValue(defaultAnalysisSettings);
    vi.spyOn(window, "scrollTo").mockImplementation(() => {});
    const fetcher = vi.fn().mockResolvedValue(new Response('{"ok":true}'));
    vi.stubGlobal("fetch", fetcher);
    setWebToken("account-b", undefined, { email: "b@example.test" });
    setWebToken("account-a", undefined, { email: "a@example.test" });
    vi.mocked(api.getSnapshot).mockResolvedValue(
      {} as Awaited<ReturnType<typeof api.getSnapshot>>,
    );
    if (modelAvailable)
      vi.mocked(api.getModelConfig).mockResolvedValue({
        provider: "openai-responses",
        model: "test",
        baseUrl: "https://api.openai.com/v1",
        hasApiKey: false,
      });
    else
      vi.mocked(api.getModelConfig).mockRejectedValue(
        new Error("model unavailable"),
      );
    try {
      render(
        <WebAccess>
          <App />
        </WebAccess>,
      );
      fireEvent.click(await screen.findByRole("button", { name: "设置" }));
      await screen.findByRole("heading", { name: "设置" });
      const account = await screen.findByRole("region", { name: "账号" });
      expect(
        within(account).getByRole("button", { name: "退出账号" }),
      ).toBeTruthy();
      fireEvent.click(
        within(account).getByRole("button", { name: "切换账号" }),
      );
      expect(
        within(account).getByRole("button", { name: "退出账号" }),
      ).toBeTruthy();
      fireEvent.click(
        within(account).getByRole("button", { name: "切换到 b@example.test" }),
      );
      await waitFor(() => expect(api.getSnapshot).toHaveBeenCalledTimes(2));
      expect(
        screen.queryByRole("complementary", { name: "主导航" }),
      ).toBeTruthy();
      expect(webToken()).toBe("account-b");
      expect(fetcher).not.toHaveBeenCalled();
      const switchedAccount = await screen.findByRole("region", {
        name: "账号",
      });
      fireEvent.click(
        within(switchedAccount).getByRole("button", { name: "退出账号" }),
      );
      await screen.findByRole("button", { name: "登录" });
      expect(webToken()).toBeUndefined();
      expect(fetcher).toHaveBeenCalledTimes(1);
      expect(fetcher.mock.calls[0][1].headers.Authorization).toBe(
        "Bearer account-b",
      );
      expect(
        screen.getByRole("button", { name: "切换到 a@example.test" }),
      ).toBeTruthy();
    } finally {
      cleanup();
      localDev.mockRestore();
      securityConfig.mockRestore();
      analysisSettings.mockRestore();
      vi.unstubAllGlobals();
      localStorage.clear();
      sessionStorage.clear();
    }
  },
);

it("falls back to questions when an old dashboard bookmark is opened", async () => {
  window.location.hash = "dashboard";
  vi.mocked(api.getSnapshot).mockResolvedValue(
    emptySnapshot as Awaited<ReturnType<typeof api.getSnapshot>>,
  );
  vi.mocked(api.getModelConfig).mockRejectedValue(
    new Error("optional model unavailable"),
  );
  render(<App />);
  const questions = await screen.findByRole("button", { name: "问答" });
  expect(questions.getAttribute("aria-current")).toBe("page");
  expect(screen.queryByRole("button", { name: "分析看板" })).toBeNull();
});

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
    "资料与复盘不受影响",
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
  expect(await screen.findByRole("heading", { name: "问答" })).toBeTruthy();
  fireEvent.change(screen.getByRole("textbox"), {
    target: { value: "暂存的问题" },
  });
  fireEvent.click(screen.getByRole("button", { name: "账号与数据" }));
  await screen.findByRole("button", { name: "测试恢复数据" });
  expect(screen.queryByRole("textbox")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "问答" }));
  expect(screen.getByDisplayValue("暂存的问题")).toBeTruthy();
});

it("shows frequently used records in primary navigation", async () => {
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
  for (const name of ["复盘记录", "参考资料"]) {
    const entry = screen.getByRole("button", { name });
    expect(entry.closest("aside")).toBeTruthy();
    expect(entry.closest("details")).toBeNull();
  }
  for (const name of ["决策确认", "记忆与规则", "组合流水", "资料与确认"])
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
  await screen.findByRole("textbox", { name: "输入问题" });
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
  await waitFor(() =>
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "关闭导航" }),
    ),
  );
  fireEvent.keyDown(window, { key: "Escape" });
  expect(
    document.querySelector("aside")?.classList.contains("mobile-open"),
  ).toBe(false);
  expect(document.activeElement).toBe(trigger);
  expect(document.querySelector("main")?.hasAttribute("inert")).toBe(false);
  expect(screen.getByRole("button", { name: "测试恢复数据" })).toBeTruthy();
});

it.each([
  ["review", "复盘记录"],
  ["evidence", "参考资料"],
])(
  "opens %s directly from primary navigation with its own active state",
  async (view, label) => {
    window.location.hash = "facts";
    vi.spyOn(window, "scrollTo").mockImplementation(() => {});
    const push = vi.spyOn(window.history, "pushState");
    vi.mocked(api.getSnapshot).mockResolvedValue(
      emptySnapshot as Awaited<ReturnType<typeof api.getSnapshot>>,
    );
    vi.mocked(api.getModelConfig).mockResolvedValue({
      provider: "openai-responses",
      model: "test",
      baseUrl: "",
      hasApiKey: false,
    });
    render(<App />);
    await screen.findByRole("heading", { name: "我的情况" });
    expect(screen.queryByText("更多资料与记录")).toBeNull();
    for (const name of ["持仓与目标", "资产概览", "组合流水", "记忆与规则"])
      expect(screen.queryByRole("button", { name })).toBeNull();
    const sidebar = screen.getByRole("complementary", { name: "主导航" });
    fireEvent.click(within(sidebar).getByRole("button", { name: label }));
    await screen.findByRole("heading", { name: label });
    expect(push).toHaveBeenCalledWith(null, "", `#${view}`);
    expect(
      within(sidebar)
        .getByRole("button", { name: label })
        .getAttribute("aria-current"),
    ).toBe("page");
    expect(
      within(sidebar)
        .getByRole("button", { name: "我的情况" })
        .hasAttribute("aria-current"),
    ).toBe(false);
    expect(document.querySelector(".sidebar button.active")?.textContent).toBe(
      label,
    );
    expect(screen.queryByRole("navigation", { name: "当前位置" })).toBeNull();
  },
);
