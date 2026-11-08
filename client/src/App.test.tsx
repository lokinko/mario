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
import * as portfolioApi from "./api/portfolio";
import * as settingsApi from "./api/settings";
import * as researchApi from "./api/research";
import * as watchApi from "./api/watch";
import emptySnapshot from "./test/emptySnapshot.json";
import { WebAccess } from "./features/account/WebAccess";
import { setWebToken, webToken } from "./lib/webSession";

vi.mock("./api/portfolio", async (original) => ({
  ...(await original<typeof import("./api/portfolio")>()),
  ensureDailyAssets: vi.fn().mockResolvedValue({
    timezone: "UTC",
    today: new Date().toISOString().slice(0, 10),
    records: [],
    nextBefore: null,
  }),
  getSnapshot: vi.fn(),
  getDailyAssets: vi.fn().mockResolvedValue({ records: [], nextBefore: null }),
  getFxSettings: vi.fn().mockResolvedValue({
    baseCurrency: "CNY",
    checkedAt: 0,
    requestedOn: "2026-10-13",
    quotes: [],
    manual: {},
    error: "",
  }),
}));
vi.mock("./api/watch", () => ({
  getDividendCalendar: vi.fn(() => new Promise(() => {})),
  getWatchBoard: vi.fn().mockResolvedValue({
    rows: [],
    weekly: [],
    weekStart: "2026-11-02",
    weekEnd: "2026-11-08",
    defaultRules: {
      minYieldBps: 500,
      daily: { reference: "lower", mode: "near", offsetBps: 200 },
      weekly: { reference: "middle", mode: "at_or_below", offsetBps: 0 },
    },
    rules: {
      minYieldBps: 500,
      daily: { reference: "lower", mode: "near", offsetBps: 200 },
      weekly: { reference: "middle", mode: "at_or_below", offsetBps: 0 },
    },
  }),
}));
vi.mock("./api/settings", async (original) => ({
  ...(await original<typeof import("./api/settings")>()),
  getModelConfig: vi.fn(),
}));
vi.mock("./api/research", async (original) => ({
  ...(await original<typeof import("./api/research")>()),
  getAnalysisHistory: vi.fn().mockResolvedValue([]),
}));
vi.mock("./api/reviews", async (original) => ({
  ...(await original<typeof import("./api/reviews")>()),
}));
vi.mock("./features/account/ServerData", async () => {
  const { AccountSettings } = await import("./features/account/WebAccess");
  return {
    ServerData: ({ onRestore }: { onRestore: () => Promise<void> }) => (
      <>
        <AccountSettings />
        <button onClick={() => void onRestore()}>测试恢复数据</button>
      </>
    ),
  };
});
beforeEach(() => {
  vi.clearAllMocks();
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  window.location.hash = "cloud";
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it("keeps exchange rates outside the main page and available across navigation", async () => {
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    emptySnapshot as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockRejectedValue(
    new Error("optional model unavailable"),
  );
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "查看汇率" }));
  const rates = await screen.findByRole("region", { name: "汇率设置" });
  expect(rates.closest("main")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "关闭详情" }));
  fireEvent.click(screen.getByRole("button", { name: "我的情况" }));
  await screen.findByRole("heading", { name: "我的情况" });
  expect(screen.getByRole("button", { name: "查看汇率" })).toBeTruthy();
});

it("switches accounts from the account and data page", async () => {
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  const fetcher = vi.fn().mockResolvedValue(new Response('{"ok":true}'));
  vi.stubGlobal("fetch", fetcher);
  setWebToken("account-b", undefined, { email: "b@example.test" });
  setWebToken("account-a", undefined, { email: "a@example.test" });
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    {} as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockResolvedValue({
    provider: "openai-responses",
    model: "test",
    baseUrl: "https://api.openai.com/v1",
    hasApiKey: false,
  });
  try {
    render(
      <WebAccess>
        <App />
      </WebAccess>,
    );
    fireEvent.click(await screen.findByRole("button", { name: "账号与数据" }));
    const account = await screen.findByRole("region", { name: "账号" });
    expect(
      within(account).getByRole("button", { name: "退出账号" }),
    ).toBeTruthy();
    fireEvent.click(within(account).getByRole("button", { name: "切换账号" }));
    expect(
      within(account).getByRole("button", { name: "退出账号" }),
    ).toBeTruthy();
    fireEvent.click(
      within(account).getByRole("button", { name: "切换到 b@example.test" }),
    );
    await waitFor(() =>
      expect(portfolioApi.getSnapshot).toHaveBeenCalledTimes(2),
    );
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
    vi.unstubAllGlobals();
    localStorage.clear();
    sessionStorage.clear();
  }
});

it.each(["dashboard", "foundation", "goals", "ledger"])(
  "falls back to buy watch for the retired %s bookmark",
  async (bookmark) => {
    window.location.hash = bookmark;
    vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
      emptySnapshot as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
    );
    vi.mocked(settingsApi.getModelConfig).mockRejectedValue(
      new Error("optional model unavailable"),
    );
    render(<App />);
    const questions = await screen.findByRole("button", { name: "买点观察" });
    expect(questions.getAttribute("aria-current")).toBe("page");
    expect(screen.queryByRole("button", { name: "分析看板" })).toBeNull();
  },
);

it("keeps local application accessible when optional model configuration fails", async () => {
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    {} as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockRejectedValue(
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
  await waitFor(() =>
    expect(portfolioApi.getSnapshot).toHaveBeenCalledTimes(2),
  );
});

it("offers retry when required local data fails to load", async () => {
  vi.mocked(portfolioApi.getSnapshot)
    .mockRejectedValueOnce(new Error("local unavailable"))
    .mockResolvedValue(
      {} as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
    );
  vi.mocked(settingsApi.getModelConfig).mockRejectedValue(
    new Error("model unavailable"),
  );
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "重新连接" }));
  expect(
    await screen.findByRole("button", { name: "测试恢复数据" }),
  ).toBeTruthy();
});

it("opens on buy watch and preserves a question draft when returning from supporting pages", async () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  window.location.hash = "";
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    emptySnapshot as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockResolvedValue({
    provider: "openai-responses",
    model: "test",
    baseUrl: "",
    hasApiKey: true,
  });
  render(<App />);
  expect(await screen.findByRole("heading", { name: "买点观察" })).toBeTruthy();
  fireEvent.click(
    within(screen.getByRole("complementary", { name: "主导航" })).getByRole(
      "button",
      { name: "问答" },
    ),
  );
  expect(await screen.findByRole("heading", { name: "问答" })).toBeTruthy();
  fireEvent.change(screen.getByRole("textbox"), {
    target: { value: "暂存的问题" },
  });
  fireEvent.click(screen.getByRole("button", { name: "账号与数据" }));
  await screen.findByRole("button", { name: "测试恢复数据" });
  expect(screen.queryByRole("textbox")).toBeNull();
  fireEvent.click(
    within(screen.getByRole("complementary", { name: "主导航" })).getByRole(
      "button",
      { name: "问答" },
    ),
  );
  expect(screen.getByDisplayValue("暂存的问题")).toBeTruthy();
  expect(confirm).not.toHaveBeenCalled();
});

it("shows frequently used records in primary navigation", async () => {
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    {} as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockResolvedValue({
    provider: "codex-bridge",
    model: "test",
    baseUrl: "codex://bridge",
    hasApiKey: true,
  });
  render(<App />);
  await screen.findAllByRole("button", { name: "问答" });
  expect(screen.getByRole("button", { name: "我的情况" })).toBeTruthy();
  for (const name of ["复盘记录", "参考资料"]) {
    const entry = screen.getByRole("button", { name });
    expect(entry.closest("aside")).toBeTruthy();
    expect(entry.closest("details")).toBeNull();
  }
  for (const name of [
    "决策确认",
    "资料与确认",
    "投资目标",
    "组合流水",
    "长期记忆",
  ])
    expect(screen.queryByRole("button", { name })).toBeNull();
});
it("responds to hash navigation without leaving the old page on screen", async () => {
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    {} as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockResolvedValue({
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

it("closes the more-navigation sheet with Escape and restores focus without changing the page", async () => {
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    {} as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockResolvedValue({
    provider: "codex-bridge",
    model: "test",
    baseUrl: "codex://bridge",
    hasApiKey: true,
  });
  render(<App />);
  const trigger = await screen.findByRole("button", { name: "更多" });
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

it("opens holdings and savings sections from the mobile tab bar", async () => {
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    emptySnapshot as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockResolvedValue({
    provider: "openai-responses",
    model: "test",
    baseUrl: "",
    hasApiKey: false,
  });
  render(<App />);
  const tabs = await screen.findByRole("navigation", { name: "快捷导航" });
  fireEvent.click(within(tabs).getByRole("button", { name: "存款" }));
  await screen.findByRole("heading", { name: "我的情况" });
  expect(
    screen
      .getByRole("tab", { name: "存款与收支" })
      .getAttribute("aria-selected"),
  ).toBe("true");
  expect(
    within(tabs)
      .getByRole("button", { name: "存款" })
      .getAttribute("aria-current"),
  ).toBe("page");
  fireEvent.click(within(tabs).getByRole("button", { name: "持仓" }));
  await waitFor(() =>
    expect(
      screen
        .getByRole("tab", { name: "投资持仓" })
        .getAttribute("aria-selected"),
    ).toBe("true"),
  );
  expect(
    within(tabs)
      .getByRole("button", { name: "存款" })
      .hasAttribute("aria-current"),
  ).toBe(false);
});

it.each([
  ["review", "复盘记录"],
  ["evidence", "参考资料"],
  ["calendar", "分红日历"],
])(
  "opens %s directly from primary navigation with its own active state",
  async (view, label) => {
    window.location.hash = "facts";
    vi.spyOn(window, "scrollTo").mockImplementation(() => {});
    const push = vi.spyOn(window.history, "pushState");
    vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
      emptySnapshot as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
    );
    vi.mocked(settingsApi.getModelConfig).mockResolvedValue({
      provider: "openai-responses",
      model: "test",
      baseUrl: "",
      hasApiKey: false,
    });
    render(<App />);
    await screen.findByRole("heading", { name: "我的情况" });
    expect(screen.queryByText("更多资料与记录")).toBeNull();
    for (const name of ["资产概览"])
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

it("composes source questions through the shell and consumes them before a data reload", async () => {
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  const evidence = vi
    .spyOn(researchApi, "getResearchEvidence")
    .mockResolvedValue([
      {
        id: "source",
        assetName: "",
        title: "黄金储备变化",
        publisher: "用户",
        sourceUrl: "",
        sourceTier: "用户资料（待核实）",
        evidenceType: "user_source_text",
        stance: "背景",
        asOfDate: "",
        claim: "原文",
        notes: "",
        active: true,
        capturedAt: "2026-10-18T00:00:00Z",
      },
    ]);
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    emptySnapshot as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockResolvedValue({
    provider: "openai-responses",
    baseUrl: "https://api.openai.com/v1",
    model: "fixture",
    hasApiKey: false,
  });
  window.location.hash = "evidence";
  try {
    render(<App />);
    const ask = await screen.findByRole("button", { name: "用资料提问" });
    await waitFor(() =>
      expect((ask as HTMLButtonElement).disabled).toBe(false),
    );
    fireEvent.click(ask);
    const question = await screen.findByRole("textbox", { name: "输入问题" });
    await waitFor(() =>
      expect((question as HTMLTextAreaElement).value).toContain("黄金储备变化"),
    );
    fireEvent.change(question, { target: { value: "继续编辑后的草稿" } });
    fireEvent.click(screen.getByRole("button", { name: "参考资料" }));
    fireEvent.click(
      within(screen.getByRole("complementary", { name: "主导航" })).getByRole(
        "button",
        { name: "问答" },
      ),
    );
    expect(
      (screen.getByRole("textbox", { name: "输入问题" }) as HTMLTextAreaElement)
        .value,
    ).toBe("继续编辑后的草稿");
    fireEvent.click(screen.getByRole("button", { name: "账号与数据" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "测试恢复数据" }),
    );
    await waitFor(() =>
      expect(portfolioApi.getSnapshot).toHaveBeenCalledTimes(2),
    );
    fireEvent.click(
      within(screen.getByRole("complementary", { name: "主导航" })).getByRole(
        "button",
        { name: "问答" },
      ),
    );
    await waitFor(() =>
      expect(
        (
          screen.getByRole("textbox", {
            name: "输入问题",
          }) as HTMLTextAreaElement
        ).value,
      ).toBe(""),
    );
  } finally {
    evidence.mockRestore();
  }
});

it("opens the investment methodology editor directly from the sidebar without a model", async () => {
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    emptySnapshot as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockRejectedValue(
    new Error("no model"),
  );
  const load = vi
    .spyOn(settingsApi, "getInvestmentMethodology")
    .mockResolvedValue({
      prompt: "我的方法论",
      defaultPrompt: "范本",
      maxLength: 4000,
    });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "投资方法论" }));
  expect(
    await screen.findByRole("heading", { name: "投资方法论" }),
  ).toBeTruthy();
  expect(
    ((await screen.findByLabelText("投资方法论正文")) as HTMLTextAreaElement)
      .value,
  ).toBe("我的方法论");
  expect(window.location.hash).toBe("#methodology");
  load.mockRestore();
});

async function editMethodologyDraft() {
  window.history.replaceState(null, "", "#methodology");
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    emptySnapshot as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockResolvedValue({
    provider: "openai-responses",
    baseUrl: "",
    model: "test-model",
    hasApiKey: true,
  });
  vi.spyOn(settingsApi, "getInvestmentMethodology").mockResolvedValue({
    prompt: "已保存的方法论",
    defaultPrompt: "范本",
    maxLength: 4000,
  });
  render(<App />);
  const input = await screen.findByLabelText("投资方法论正文");
  fireEvent.change(input, { target: { value: "尚未保存的方法论草稿" } });
  return input;
}

it("keeps a methodology draft when sidebar navigation is declined, then discards it only after confirmation", async () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  const input = await editMethodologyDraft();
  fireEvent.click(screen.getByRole("button", { name: "买点观察" }));
  expect(confirm).toHaveBeenCalledTimes(1);
  expect(screen.getByLabelText("投资方法论正文")).toBe(input);
  expect((input as HTMLTextAreaElement).value).toBe("尚未保存的方法论草稿");
  expect(window.location.hash).toBe("#methodology");
  confirm.mockReturnValue(true);
  fireEvent.click(screen.getByRole("button", { name: "买点观察" }));
  await screen.findByRole("heading", { name: "买点观察" });
  expect(confirm).toHaveBeenCalledTimes(2);
  expect(window.location.hash).toBe("#watch");
});

it.each([false, true])(
  "handles paired history events once when discarding a draft is %s",
  async (accepted) => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(accepted);
    await editMethodologyDraft();
    window.history.replaceState(null, "", "#cloud");
    fireEvent(window, new PopStateEvent("popstate"));
    fireEvent(window, new HashChangeEvent("hashchange"));
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(window.location.hash).toBe(accepted ? "#cloud" : "#methodology");
    if (accepted) await screen.findByRole("button", { name: "测试恢复数据" });
    else expect(screen.getByDisplayValue("尚未保存的方法论草稿")).toBeTruthy();
  },
);

it("keeps the draft guard when selecting the current page again", async () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  await editMethodologyDraft();
  fireEvent.click(screen.getByRole("button", { name: "投资方法论" }));
  expect(confirm).not.toHaveBeenCalled();
  expect(screen.getByDisplayValue("尚未保存的方法论草稿")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "买点观察" }));
  expect(confirm).toHaveBeenCalledTimes(1);
  expect(screen.getByDisplayValue("尚未保存的方法论草稿")).toBeTruthy();
});

it("preserves the income draft guard when a mobile tab changes only the facts section", async () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  window.history.replaceState(null, "", "#facts");
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    emptySnapshot as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockRejectedValue(
    new Error("no model"),
  );
  render(<App />);
  fireEvent.click(await screen.findByRole("tab", { name: "存款与收支" }));
  fireEvent.change(screen.getByRole("spinbutton", { name: /月收入/ }), {
    target: { value: "12345" },
  });
  const mobile = screen.getByRole("navigation", { name: "快捷导航" });
  fireEvent.click(within(mobile).getByRole("button", { name: "存款" }));
  expect(confirm).not.toHaveBeenCalled();
  expect(
    (screen.getByRole("spinbutton", { name: /月收入/ }) as HTMLInputElement)
      .value,
  ).toBe("12345");
  fireEvent.click(screen.getByRole("button", { name: "投资方法论" }));
  expect(confirm).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("heading", { name: "我的情况" })).toBeTruthy();
});

it("does not treat buy-watch search and sort changes as unsaved input", async () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  window.history.replaceState(null, "", "#watch");
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(
    emptySnapshot as Awaited<ReturnType<typeof portfolioApi.getSnapshot>>,
  );
  vi.mocked(settingsApi.getModelConfig).mockRejectedValue(
    new Error("no model"),
  );
  const emptyBoard = await watchApi.getWatchBoard();
  vi.mocked(watchApi.getWatchBoard).mockResolvedValueOnce({
    ...emptyBoard,
    rows: [
      {
        entry: {
          symbol: "600036.SS",
          name: "招商银行",
          thesis: "长期持有",
          createdAt: "2026-11-01T00:00:00Z",
          pinned: false,
        },
        latest: null,
        checkedAt: null,
        refreshFailed: false,
        alert: false,
        dividendPrices: [],
      },
    ],
  });
  render(<App />);
  fireEvent.change(
    await screen.findByRole("textbox", { name: "搜索观察公司" }),
    { target: { value: "招商" } },
  );
  fireEvent.change(screen.getByRole("combobox", { name: "观察排序" }), {
    target: { value: "name" },
  });
  fireEvent.click(screen.getByRole("button", { name: "投资方法论" }));
  expect(confirm).not.toHaveBeenCalled();
  await screen.findByRole("heading", { name: "投资方法论" });
});
