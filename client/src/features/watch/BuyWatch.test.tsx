// @vitest-environment jsdom
import { defaultRules } from "../../test/watchRules";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { BuyWatch } from "./BuyWatch";
import * as api from "../../api/watch";
import type { Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";
import catalogFixture from "../../../../server/data/watch-catalog.json";

vi.mock("../../api/watch", () => ({
  getWatchBoard: vi.fn(),
  getWatchDetail: vi.fn(),
  getWatchCatalogDetail: vi.fn(),
  getWatchCatalog: vi.fn(),
  getDividendCalendar: vi.fn(),
  saveWatchCatalog: vi.fn(),
  resolveCatalogCompany: vi.fn(),
  refreshWatchCatalog: vi.fn(),
  saveWatch: vi.fn(),
  saveWatchGrid: vi.fn(),
  previewWatchGrid: vi.fn(),
  setWatchPinned: vi.fn(),
  deleteWatch: vi.fn(),
  readWatchAlerts: vi.fn(),
  refreshWatch: vi.fn(),
  saveWatchRules: vi.fn(),
}));
const empty: api.WatchBoard = {
  rows: [],
  weekly: [],
  weekStart: "2026-11-02",
  weekEnd: "2026-11-08",
  rules: defaultRules,
  defaultRules,
};
const row: api.WatchRow = {
  dividendPrices: [
    { yieldPct: 4, price: "125.00", reached: true, changePct: 25 },
    { yieldPct: 5, price: "100.00", reached: true, changePct: 0 },
    { yieldPct: 6, price: "83.33", reached: false, changePct: -16.67 },
    { yieldPct: 7, price: "71.42", reached: false, changePct: -28.58 },
  ],
  entry: {
    symbol: "600036.SS",
    name: "测试公司",
    thesis: "关注分红的持续性",
    createdAt: "2026-11-04T12:00:00Z",
    pinned: false,
  },
  latest: {
    rules: defaultRules,
    yieldMatchesRule: true,
    symbol: "600036.SS",
    observedOn: "2026-11-04",
    checkedAt: "2026-11-04T12:00:00Z",
    price: 100,
    dividendPerShare: "5",
    yieldPct: 5,
    daily: {
      lower: 100,
      middle: 102,
      upper: 104,
      distancePct: 0,
      reference: "lower",
      matchesRule: true,
    },
    weekly: {
      lower: 100,
      middle: 103,
      upper: 106,
      distancePct: (100 / 103 - 1) * 100,
      reference: "middle",
      matchesRule: true,
    },
    status: "attention",
    issues: [],
    sources: ["https://example.test/source"],
    dividendEvents: [],
  },
  alert: true,
  checkedAt: "2026-11-04T12:00:00Z",
  refreshFailed: false,
};
const props = () => ({
  snapshot: fixture as unknown as Snapshot,
  onAsk: vi.fn(),
  flash: vi.fn(),
  onEdited: vi.fn(),
  onSaved: vi.fn(),
});
const catalog: api.CatalogBoard = {
  revision: 1,
  canEdit: false,
  catalog: catalogFixture,
  rows: catalogFixture.industries.flatMap((industry) =>
    industry.companies.map((company) => ({
      company,
      industryId: industry.id,
      checkedFor: "2026-11-04",
      latest: { ...row.latest!, symbol: company.symbol },
      dividendPrices: row.dividendPrices,
      checkedAt: row.checkedAt,
      refreshFailed: false,
    })),
  ),
  rules: defaultRules,
};
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.getWatchDetail).mockImplementation(async (symbol) => {
    const values = vi.mocked(api.getWatchBoard).mock.settledResults;
    const board = values[values.length - 1]?.value as
      api.WatchBoard | undefined;
    return structuredClone(
      board?.rows.find((r) => r.entry.symbol === symbol) ?? row,
    );
  });
  vi.mocked(api.getWatchCatalogDetail).mockImplementation(async (symbol) => {
    const values = vi.mocked(api.getWatchCatalog).mock.settledResults;
    const board = values[values.length - 1]?.value as
      api.CatalogBoard | undefined;
    return structuredClone(
      (board ?? catalog).rows.find((r) => r.company.symbol === symbol)!,
    );
  });
  vi.mocked(api.getWatchBoard).mockResolvedValue(structuredClone(empty));
  vi.mocked(api.getDividendCalendar).mockResolvedValue({
    start: "2026-11-07",
    end: "2026-12-06",
    asOf: "2026-11-07",
    scope: "all",
    entries: [],
    coverage: [],
  });
  vi.mocked(api.getWatchCatalog).mockResolvedValue(structuredClone(catalog));
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it("defaults to personal watches and switches all industry groups without private controls", async () => {
  vi.mocked(api.getWatchBoard).mockResolvedValue({ ...empty, rows: [row] });
  render(<BuyWatch {...props()} />);
  expect(await screen.findByRole("tab", { name: "自选 1" })).toHaveProperty(
    "ariaSelected",
    "true",
  );
  expect(await screen.findAllByRole("tab")).toHaveLength(
    catalog.catalog.industries.length + 1,
  );
  for (const industry of catalog.catalog.industries) {
    fireEvent.click(
      screen.getByRole("tab", {
        name: `${industry.name} ${industry.companies.length}`,
      }),
    );
    const table = screen.getByRole("table", { name: "观察公司对比" });
    expect(within(table).getAllByRole("row")).toHaveLength(
      industry.companies.length + 1,
    );
    expect(screen.queryByRole("button", { name: /置顶/ })).toBeNull();
  }
  fireEvent.click(screen.getByRole("tab", { name: "自选 1" }));
  expect(screen.queryByRole("button", { name: /周末复盘/ })).toBeNull();
  expect(screen.getByRole("button", { name: /置顶测试公司/ })).toBeTruthy();
  expect(api.saveWatch).not.toHaveBeenCalled();
  expect(api.refreshWatchCatalog).not.toHaveBeenCalled();
});

it("clears search and filters on group changes while keeping sorting and cards", async () => {
  render(<BuyWatch {...props()} />);
  fireEvent.click(await screen.findByRole("tab", { name: "银行 11" }));
  fireEvent.change(screen.getByRole("textbox", { name: "搜索观察公司" }), {
    target: { value: "招商" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "持仓筛选" }), {
    target: { value: "held" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "观察排序" }), {
    target: { value: "yield-asc" },
  });
  fireEvent.click(screen.getByRole("button", { name: "详细卡片" }));
  expect(screen.getByText("当前条件下没有公司")).toBeTruthy();
  fireEvent.click(screen.getByRole("tab", { name: "能源 6" }));
  expect(screen.getByRole("textbox", { name: "搜索观察公司" })).toHaveProperty(
    "value",
    "",
  );
  expect(screen.getByRole("combobox", { name: "持仓筛选" })).toHaveProperty(
    "value",
    "all",
  );
  expect(screen.getByRole("combobox", { name: "观察排序" })).toHaveProperty(
    "value",
    "yield-asc",
  );
  expect(screen.getByRole("button", { name: "详细卡片" })).toHaveProperty(
    "ariaPressed",
    "true",
  );
  expect(screen.getAllByRole("button", { name: "查看依据" })).toHaveLength(6);
  expect(screen.queryByRole("button", { name: "结合我的交易讨论" })).toBeNull();
});

it("adds from an industry once without overwriting existing personal reasons or copying public history", async () => {
  const personal = {
    ...row,
    entry: { ...row.entry, thesis: "我的独立关注理由", pinned: true },
  };
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [personal],
  });
  const company = catalog.rows.find(
    (r) => r.company.symbol === "601398.SS",
  )!.company;
  vi.mocked(api.saveWatch).mockResolvedValue({
    ...row.entry,
    ...company,
    thesis: "",
    pinned: false,
  });
  vi.mocked(api.getWatchBoard)
    .mockResolvedValueOnce({ ...empty, rows: [personal] })
    .mockResolvedValue({
      ...empty,
      rows: [
        personal,
        {
          ...row,
          entry: { ...row.entry, ...company, thesis: "", pinned: false },
          latest: null,
          dividendPrices: [],
        },
      ],
    });
  render(<BuyWatch {...props()} />);
  fireEvent.click(await screen.findByRole("tab", { name: "银行 11" }));
  expect(
    screen.getByRole("button", { name: "已在自选招商银行" }),
  ).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", { name: /招商银行.*600036/ }));
  expect(
    within(screen.getByRole("dialog")).queryByRole("button", { name: /自选/ }),
  ).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "关闭详情" }));
  fireEvent.click(screen.getByRole("button", { name: "加入自选工商银行" }));
  expect(
    await screen.findByRole("button", { name: "已在自选工商银行" }),
  ).toHaveProperty("disabled", true);
  expect(api.saveWatch).toHaveBeenCalledExactlyOnceWith(
    {
      symbol: company.symbol,
      name: company.name,
      thesis: "",
    },
    true,
  );
  fireEvent.click(screen.getByRole("tab", { name: "自选 2" }));
  expect(screen.getByRole("button", { name: "取消置顶测试公司" })).toBeTruthy();
  expect(screen.getByText("等待首次检查")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /测试公司.*600036/ }));
  expect(screen.getByText("我的独立关注理由")).toBeTruthy();
});

it("keeps public details separate and refreshes only the selected industry", async () => {
  vi.mocked(api.refreshWatchCatalog).mockRejectedValue(
    new Error("行业来源暂不可用"),
  );
  render(<BuyWatch {...props()} />);
  fireEvent.click(await screen.findByRole("tab", { name: "银行 11" }));
  fireEvent.click(screen.getByRole("button", { name: /招商银行.*600036/ }));
  const drawer = await screen.findByRole("dialog");
  expect(within(drawer).queryByText("公司公开报告")).toBeNull();
  expect(within(drawer).queryByText(/600036\.SS ·/)).toBeNull();
  expect(within(drawer).queryByText("已有持仓与最近交易")).toBeNull();
  expect(
    within(drawer).queryByRole("button", { name: "编辑观察资料" }),
  ).toBeNull();
  fireEvent.click(within(drawer).getByRole("button", { name: "重新检查数据" }));
  expect(await within(drawer).findByText(/行业来源暂不可用/)).toBeTruthy();
  expect(api.refreshWatchCatalog).toHaveBeenCalledExactlyOnceWith("banks");
  expect(within(drawer).getByText("收盘价 · 2026-11-04")).toBeTruthy();
  expect(api.refreshWatch).not.toHaveBeenCalled();
});

it("enforces the personal limit in industry lists and drawers", async () => {
  const full = Array.from({ length: 50 }, (_, i) => ({
    ...row,
    entry: {
      ...row.entry,
      symbol: `600${String(i + 100).padStart(3, "0")}.SS`,
    },
  }));
  vi.mocked(api.getWatchBoard).mockResolvedValue({ ...empty, rows: full });
  render(<BuyWatch {...props()} />);
  fireEvent.click(await screen.findByRole("tab", { name: "银行 11" }));
  expect(
    screen.getByRole("button", { name: "加入自选招商银行" }),
  ).toHaveProperty("disabled", true);
  expect(screen.getByText(/自选已达 50 家/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /招商银行.*600036/ }));
  expect(
    within(screen.getByRole("dialog")).getByRole("button", {
      name: "加入自选",
    }),
  ).toHaveProperty("disabled", true);
  expect(api.saveWatch).not.toHaveBeenCalled();
});

it("allows existing watches when the catalogue fails and can retry its read independently", async () => {
  vi.mocked(api.getWatchBoard).mockResolvedValue({ ...empty, rows: [row] });
  vi.mocked(api.getWatchCatalog)
    .mockRejectedValueOnce(new Error("目录不可用"))
    .mockResolvedValue(catalog);
  render(<BuyWatch {...props()} />);
  expect(
    await screen.findByRole("button", { name: /置顶测试公司/ }),
  ).toBeTruthy();
  fireEvent.click(
    await screen.findByRole("button", { name: "重新读取行业目录" }),
  );
  expect(await screen.findByRole("tab", { name: "银行 11" })).toBeTruthy();
  expect(api.getWatchBoard).toHaveBeenCalledTimes(1);
});

it("adds an unheld company without creating a financial position", async () => {
  const p = props();
  render(<BuyWatch {...p} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "加入第一家公司" }),
  );
  fireEvent.change(screen.getByLabelText("证券代码"), {
    target: { value: "600036.SS" },
  });
  fireEvent.change(screen.getByLabelText("公司名称"), {
    target: { value: "测试公司" },
  });
  fireEvent.change(screen.getByLabelText("我认可它的理由"), {
    target: { value: "关注分红" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存观察公司" }));
  await waitFor(() =>
    expect(api.saveWatch).toHaveBeenCalledWith({
      symbol: "600036.SS",
      name: "测试公司",
      thesis: "关注分红",
    }),
  );
  expect(p.onEdited).toHaveBeenCalled();
  await waitFor(() => expect(p.onSaved).toHaveBeenCalled());
});

it("keeps the entry editable after save failure", async () => {
  vi.mocked(api.saveWatch).mockRejectedValue(new Error("写入失败"));
  render(<BuyWatch {...props()} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "加入第一家公司" }),
  );
  fireEvent.change(screen.getByLabelText("证券代码"), {
    target: { value: "600036.SS" },
  });
  fireEvent.change(screen.getByLabelText("公司名称"), {
    target: { value: "测试公司" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存观察公司" }));
  await screen.findByText(/写入失败/);
  expect((screen.getByLabelText("公司名称") as HTMLInputElement).value).toBe(
    "测试公司",
  );
  expect(screen.getByRole("dialog")).toBeTruthy();
});

it("opens grounded discussion, acknowledges alerts and preserves missing-data status", async () => {
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [structuredClone(row)],
  });
  const p = props();
  render(<BuyWatch {...p} />);
  await screen.findByText("进入关注区间");
  fireEvent.click(
    within(screen.getByRole("table", { name: "观察公司对比" })).getByRole(
      "button",
      { name: /^测试公司/ },
    ),
  );
  fireEvent.click(screen.getByRole("button", { name: "结合我的交易讨论" }));
  expect(p.onAsk).toHaveBeenCalledWith(expect.stringContaining("600036.SS"));
  expect(p.onAsk).toHaveBeenCalledWith(expect.stringContaining("最近真实交易"));
  expect(p.onAsk).toHaveBeenCalledWith(
    expect.stringContaining("周线BOLL(20,2)不高于中轨"),
  );
  fireEvent.click(screen.getByRole("button", { name: "标为已读" }));
  await waitFor(() =>
    expect(api.readWatchAlerts).toHaveBeenCalledWith("600036.SS"),
  );
  const missing = structuredClone(row);
  missing.latest!.status = "incomplete";
  missing.latest!.weekly = null;
  missing.latest!.issues = ["weekly_history_incomplete"];
  vi.mocked(api.refreshWatch).mockResolvedValue({ ...empty, rows: [missing] });
  await waitFor(() =>
    expect(
      screen
        .getByRole("button", { name: "检查最新收盘" })
        .hasAttribute("disabled"),
    ).toBe(false),
  );
  fireEvent.click(screen.getByRole("button", { name: "检查最新收盘" }));
  await screen.findByText("待补齐数据");
  expect(screen.queryByText("进入关注区间")).toBeNull();
  expect(screen.getByRole("button", { name: "重新检查数据" })).toBeTruthy();
});

it("shows the exact dividend events behind the closing yield", async () => {
  const stored = structuredClone(row);
  stored.latest!.dividendBasis = {
    asOf: "2026-11-04",
    fetchedAt: "2026-11-04T10:00:00Z",
  };
  stored.latest!.dividendEvents = [
    {
      reportDate: "2025-12-31",
      exDate: "2026-07-01",
      cashPerTen: "12.345",
      bonusPerTen: "0",
      transferPerTen: "2",
    },
  ];
  vi.mocked(api.getWatchBoard).mockResolvedValue({ ...empty, rows: [stored] });
  render(<BuyWatch {...props()} />);
  fireEvent.click(
    within(
      await screen.findByRole("table", { name: "观察公司对比" }),
    ).getByRole("button", { name: /^测试公司/ }),
  );
  const event = (await screen.findByText(/除息日 2026-07-01/)).closest("tr")!;
  expect(event.textContent).toContain("12.345");
  expect(event.textContent).toContain("报告期 2025-12-31");
});

it("ranks and highlights met conditions while search and holdings filter the list", async () => {
  const missing = structuredClone(row);
  missing.entry = { ...missing.entry, symbol: "600000.SS", name: "资料待查" };
  missing.latest = null;
  missing.dividendPrices = [];
  missing.alert = false;
  const waiting = structuredClone(row);
  waiting.entry = { ...waiting.entry, symbol: "000001.SZ", name: "另一家公司" };
  waiting.alert = false;
  waiting.latest!.status = "waiting";
  waiting.latest!.yieldPct = 4;
  waiting.latest!.yieldMatchesRule = false;
  waiting.dividendPrices.find((p) => p.yieldPct === 5)!.reached = false;
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [missing, waiting, structuredClone(row)],
  });
  render(<BuyWatch {...props()} />);
  const table = await screen.findByRole("table", { name: "观察公司对比" });
  const records = within(table).getAllByRole("row").slice(1);
  expect(
    records.map((r) => r.querySelector(".watch-company strong")?.textContent),
  ).toEqual(["测试公司", "另一家公司", "资料待查"]);
  const yieldCell = (r: HTMLElement) => r.querySelector("td.rule-yield")!;
  expect(yieldCell(records[0]).classList.contains("watch-met")).toBe(true);
  expect(yieldCell(records[1]).classList.contains("watch-met")).toBe(false);
  fireEvent.change(screen.getByRole("textbox", { name: "搜索观察公司" }), {
    target: { value: "不存在" },
  });
  expect(screen.queryByRole("table", { name: "观察公司对比" })).toBeNull();
  fireEvent.click(screen.getAllByRole("button", { name: "清除筛选" })[0]);
  fireEvent.change(screen.getByRole("combobox", { name: "持仓筛选" }), {
    target: { value: "held" },
  });
  expect(screen.queryByRole("table", { name: "观察公司对比" })).toBeNull();
  expect(api.saveWatch).not.toHaveBeenCalled();
  expect(api.deleteWatch).not.toHaveBeenCalled();
});

it("sorts missing metrics last and switches between compact and visual details", async () => {
  const missing = structuredClone(row);
  missing.entry = { ...missing.entry, symbol: "600000.SS", name: "未知公司" };
  missing.latest = null;
  missing.dividendPrices = [];
  missing.alert = false;
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [missing, structuredClone(row)],
    weekly: [row.latest!],
  });
  render(<BuyWatch {...props()} />);
  await screen.findByRole("table", { name: "观察公司对比" });
  fireEvent.change(screen.getByRole("combobox", { name: "观察排序" }), {
    target: { value: "yield" },
  });
  const records = within(
    screen.getByRole("table", { name: "观察公司对比" }),
  ).getAllByRole("row");
  expect(records[1].textContent).toContain("测试公司");
  expect(records[2].textContent).toContain("未知公司");
  fireEvent.click(screen.getByRole("button", { name: "详细卡片" }));
  expect(screen.queryByRole("table", { name: "观察公司对比" })).toBeNull();
  expect(screen.getAllByRole("img", { name: /日线下轨/ })).toHaveLength(1);
  expect(screen.getByText("¥83.33")).toBeTruthy();
  expect(
    screen.getByText("暂无可用参考价：需要有效且为正的 TTM 每股分红。"),
  ).toBeTruthy();
  expect(api.refreshWatch).not.toHaveBeenCalled();
});

it("sorts all four metrics in both directions from the table headers", async () => {
  const cheaper = structuredClone(row);
  cheaper.entry = { ...cheaper.entry, symbol: "600000.SS", name: "低价公司" };
  cheaper.latest!.price = 10;
  cheaper.latest!.yieldPct = 0;
  cheaper.latest!.yieldMatchesRule = false;
  cheaper.latest!.daily!.distancePct = -3;
  cheaper.latest!.weekly!.distancePct = 1;
  const missing = {
    ...row,
    entry: { ...row.entry, symbol: "000001.SZ", name: "未知公司" },
    latest: null,
  };
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [missing, cheaper, row],
  });
  render(<BuyWatch {...props()} />);
  const table = await screen.findByRole("table", { name: "观察公司对比" });
  for (const [label, first, second] of [
    ["收盘价", "低价公司", "测试公司"],
    ["TTM 股息率", "测试公司", "低价公司"],
    ["距日线下轨", "测试公司", "低价公司"],
    ["距周线中轨", "低价公司", "测试公司"],
  ]) {
    const header = within(table).getByRole("button", {
      name: new RegExp(`^${label}：`),
    });
    for (const expected of [
      [first, second],
      [second, first],
    ]) {
      fireEvent.click(header);
      const records = within(table).getAllByRole("row").slice(1);
      [...expected, "未知公司"].forEach((name, i) => {
        expect(within(records[i]).getByText(name)).toBeTruthy();
      });
    }
  }
});

it("puts a pinned company first in both views without overriding filters", async () => {
  const other = structuredClone(row);
  other.entry = { ...other.entry, symbol: "600000.SS", name: "重点公司" };
  other.latest!.yieldPct = 2;
  other.latest!.yieldMatchesRule = false;
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [row, other],
  });
  vi.mocked(api.setWatchPinned).mockImplementation(async (_symbol, pinned) => ({
    ...other.entry,
    pinned,
  }));
  render(<BuyWatch {...props()} />);
  const table = await screen.findByRole("table", { name: "观察公司对比" });
  fireEvent.change(screen.getByRole("combobox", { name: "观察排序" }), {
    target: { value: "yield" },
  });
  expect(within(table).getAllByRole("row")[1].textContent).toContain(
    "测试公司",
  );
  fireEvent.click(within(table).getByRole("button", { name: "置顶重点公司" }));
  await within(table).findByRole("button", { name: "取消置顶重点公司" });
  expect(api.setWatchPinned).toHaveBeenCalledWith("600000.SS", true);
  expect(within(table).getAllByRole("row")[1].textContent).toContain(
    "重点公司",
  );
  fireEvent.change(screen.getByRole("textbox", { name: "搜索观察公司" }), {
    target: { value: "测试公司" },
  });
  expect(within(table).queryByText("重点公司")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "清除筛选" }));
  fireEvent.click(screen.getByRole("button", { name: "详细卡片" }));
  expect(screen.getAllByRole("article")[0].textContent).toContain("重点公司");
  fireEvent.click(screen.getByRole("button", { name: "取消置顶重点公司" }));
  await screen.findByRole("button", { name: "置顶重点公司" });
  expect(api.setWatchPinned).toHaveBeenLastCalledWith("600000.SS", false);
  expect(screen.getAllByRole("article")[0].textContent).toContain("测试公司");
});

it("keeps the saved pin state when the write fails and sends only editable fields on edit", async () => {
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [{ ...row, entry: { ...row.entry, pinned: true } }],
  });
  vi.mocked(api.setWatchPinned).mockRejectedValue(new Error("未能取消置顶"));
  render(<BuyWatch {...props()} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "取消置顶测试公司" }),
  );
  await screen.findByText(/未能取消置顶/);
  expect(screen.getByRole("button", { name: "取消置顶测试公司" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "详细卡片" }));
  fireEvent.click(screen.getByRole("button", { name: "编辑" }));
  fireEvent.change(screen.getByLabelText("公司名称"), {
    target: { value: "更新名称" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存观察公司" }));
  await waitFor(() =>
    expect(api.saveWatch).toHaveBeenCalledWith({
      symbol: row.entry.symbol,
      name: "更新名称",
      thesis: row.entry.thesis,
    }),
  );
});

it("saves a price grid, keeps failed drafts and shows the budget share", async () => {
  vi.mocked(api.getWatchBoard).mockResolvedValue({ ...empty, rows: [row] });
  vi.mocked(api.previewWatchGrid).mockReturnValue(new Promise(() => {}));
  const grid: api.WatchGridPlan = {
    budget: "10000",
    basePrice: "100",
    sizeUnit: "budgetPct",
    levels: [{ price: "90", size: "50" }],
  };
  vi.mocked(api.saveWatchGrid)
    .mockRejectedValueOnce(new Error("暂时无法保存网格"))
    .mockResolvedValue({
      ...empty,
      rows: [
        {
          ...row,
          entry: { ...row.entry, gridPlan: grid },
          gridProjection: {
            topPrice: null,
            levels: [
              {
                price: "90.00",
                side: "buy",
                targetAmount: "5000.00",
                shares: 0,
                amount: "0.00",
                cumulativeAmount: "0.00",
                budgetPct: "0.0000",
              },
            ],
            scenarios: [],
          },
        },
      ],
    });
  render(<BuyWatch {...props()} />);
  fireEvent.click(
    within(
      await screen.findByRole("table", { name: "观察公司对比" }),
    ).getByRole("button", { name: /^测试公司/ }),
  );
  fireEvent.click(screen.getByRole("button", { name: "设置网格方案" }));
  const change = (label: string | RegExp, value: string) =>
    fireEvent.change(screen.getByLabelText(label), { target: { value } });
  change("标的配置金额（元）", "10000");
  change(/^基准价（元）/, "100");
  fireEvent.click(screen.getByRole("button", { name: /添加/ }));
  change("第 1 档价格", "90");
  change("第 1 档规模", "50");
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await screen.findByText(/暂时无法保存网格/);
  expect((screen.getByLabelText("第 1 档规模") as HTMLInputElement).value).toBe(
    "50",
  );
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  const levels = await screen.findByRole("table", { name: "网格档位" });
  expect(api.saveWatchGrid).toHaveBeenLastCalledWith(row.entry.symbol, grid);
  expect(within(levels).getByText("不足一手")).toBeTruthy();
  expect(within(levels).getByText("0%")).toBeTruthy();
});

it("shows removal failures inside the company drawer and preserves the company", async () => {
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [structuredClone(row)],
  });
  vi.mocked(api.deleteWatch).mockRejectedValue(new Error("删除未完成"));
  vi.spyOn(window, "confirm").mockReturnValue(true);
  render(<BuyWatch {...props()} />);
  fireEvent.click(
    within(
      await screen.findByRole("table", { name: "观察公司对比" }),
    ).getByRole("button", { name: /^测试公司/ }),
  );
  fireEvent.click(screen.getByRole("button", { name: "移出观察名单" }));
  await within(screen.getByRole("dialog")).findByText(/删除未完成/);
  expect(api.deleteWatch).toHaveBeenCalledWith("600036.SS");
  expect(screen.getByRole("dialog")).toBeTruthy();
});

it("matches current holdings by canonical code and excludes closed positions", async () => {
  const closed = structuredClone(row);
  closed.entry = { ...closed.entry, symbol: "000001.SZ", name: "已清仓公司" };
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [structuredClone(row), closed],
  });
  const p = props();
  p.snapshot = structuredClone(p.snapshot);
  const holding = {
    id: "held",
    symbol: "600036.SH",
    name: "测试持仓",
    assetClass: "股票" as const,
    marketValue: 100,
    costBasis: 100,
    quantity: 1,
    targetPct: 0,
    currency: "CNY",
    fxRateToBase: 1,
    valuationDate: "2026-11-04",
    fxRateSource: "manual",
    fxRateObservedOn: "2026-11-04",
  };
  p.snapshot.holdings = [
    holding,
    { ...holding, id: "closed", symbol: "000001.SZ", quantity: 0 },
  ];
  render(<BuyWatch {...p} />);
  await screen.findByRole("table", { name: "观察公司对比" });
  fireEvent.change(screen.getByRole("combobox", { name: "持仓筛选" }), {
    target: { value: "held" },
  });
  const table = screen.getByRole("table", { name: "观察公司对比" });
  expect(within(table).getAllByRole("row")).toHaveLength(2);
  expect(within(table).getByText("测试公司")).toBeTruthy();
  expect(within(table).queryByText("已清仓公司")).toBeNull();
});

it("uses exact server threshold results instead of rounding the displayed yield into a pass", async () => {
  const boundary = structuredClone(row);
  boundary.latest!.status = "waiting";
  boundary.latest!.yieldPct = 4.999999;
  boundary.latest!.yieldMatchesRule = false;
  boundary.dividendPrices.find((p) => p.yieldPct === 5)!.reached = false;
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [boundary],
  });
  render(<BuyWatch {...props()} />);
  const table = await screen.findByRole("table", { name: "观察公司对比" });
  expect(table.querySelector("td.rule-yield.watch-met")).toBeNull();
});

it("ranks by the weekly middle and shades that range even far from the weekly lower", async () => {
  const below = structuredClone(row);
  below.latest!.weekly!.lower = 50;
  const above = structuredClone(row);
  above.entry = { ...above.entry, symbol: "000001.SZ", name: "周线未达标" };
  above.latest!.weekly!.distancePct = 0.000001;
  above.latest!.weekly!.matchesRule = false;
  above.latest!.status = "waiting";
  above.alert = false;
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [below, above],
  });
  render(<BuyWatch {...props()} />);
  const table = await screen.findByRole("table", { name: "观察公司对比" });
  expect(
    within(table).getByRole("button", { name: /^距周线中轨：/ }),
  ).toBeTruthy();
  const [first, second] = within(table).getAllByRole("row").slice(1);
  expect(first.querySelector("td.rule-weekly.watch-met")).toBeTruthy();
  expect(second.textContent).toContain("周线未达标");
  expect(second.querySelector("td.rule-weekly.watch-met")).toBeNull();
  fireEvent.change(screen.getByRole("textbox", { name: "搜索观察公司" }), {
    target: { value: "测试公司" },
  });
  fireEvent.click(screen.getByRole("button", { name: "详细卡片" }));
  expect(screen.getByText("周线不高于中轨")).toBeTruthy();
  const track = screen.getByRole("img", { name: /周线下轨.*距中轨/ });
  const middle = parseFloat(track.style.getPropertyValue("--middle"));
  const start = parseFloat(track.style.getPropertyValue("--zone-start"));
  const width = parseFloat(track.style.getPropertyValue("--zone-width"));
  expect(start + width).toBeCloseTo(middle);
});

it("shows public maintenance only for the server-authorized superuser", async () => {
  const p = props();
  const view = render(<BuyWatch {...p} />);
  await screen.findByRole("tab", { name: "银行 11" });
  expect(screen.queryByRole("button", { name: "维护公共公司" })).toBeNull();
  view.unmount();
  vi.mocked(api.getWatchCatalog).mockResolvedValue({
    ...catalog,
    canEdit: true,
  });
  render(<BuyWatch {...p} />);
  fireEvent.click(await screen.findByRole("button", { name: "维护公共公司" }));
  expect(screen.getByRole("dialog", { name: "维护公共公司目录" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "关闭详情" }));
  expect(p.onSaved).toHaveBeenCalled();
});

it("applies saved account rules to summaries, reference prices, both periods and discussions", async () => {
  const p = props();
  const custom: api.WatchRules = {
    minYieldBps: 333,
    daily: { reference: "upper", mode: "near", offsetBps: 125 },
    weekly: { reference: "lower", mode: "at_or_above", offsetBps: -250 },
  };
  const updated = structuredClone(row);
  updated.latest!.rules = custom;
  updated.latest!.daily!.reference = "upper";
  updated.latest!.weekly!.reference = "lower";
  updated.dividendPrices.push({
    yieldPct: 3.33,
    price: "150.15",
    reached: true,
    changePct: 50.15,
  });
  vi.mocked(api.getWatchBoard).mockResolvedValue({ ...empty, rows: [row] });
  vi.mocked(api.saveWatchRules).mockResolvedValue({
    ...empty,
    rules: custom,
    rows: [updated],
  });
  render(<BuyWatch {...p} />);
  const saved = await screen.findByRole("group", { name: "观察规则" });
  fireEvent.click(
    within(saved).getByRole("button", { name: /① TTM 税前股息/ }),
  );
  fireEvent.change(await screen.findByLabelText("最低税前 TTM 股息率（%）"), {
    target: { value: "3.33" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存规则" }));
  await waitFor(() =>
    expect(screen.queryByLabelText("最低税前 TTM 股息率（%）")).toBeNull(),
  );
  expect(screen.getByText("① TTM 税前股息 ≥3.33%")).toBeTruthy();
  expect(
    screen.getByRole("columnheader", { name: "3.33% 参考价" }),
  ).toBeTruthy();
  expect(screen.getByRole("columnheader", { name: /距日线上轨/ })).toBeTruthy();
  expect(screen.getByRole("columnheader", { name: /距周线下轨/ })).toBeTruthy();
  expect(screen.getByText("150.15")).toBeTruthy();
  fireEvent.click(
    within(screen.getByRole("table", { name: "观察公司对比" })).getByRole(
      "button",
      { name: /^测试公司/ },
    ),
  );
  fireEvent.click(screen.getByRole("button", { name: "结合我的交易讨论" }));
  expect(p.onAsk).toHaveBeenCalledWith(
    expect.stringContaining("股息率至少3.33%"),
  );
  expect(p.onAsk).toHaveBeenCalledWith(
    expect.stringContaining("日线BOLL(20,2)上轨 ±1.25%"),
  );
});

it("shows personal summaries before the catalogue finishes and reads full evidence only on opening details", async () => {
  let finishBoard!: (value: api.WatchBoard) => void;
  vi.mocked(api.getWatchBoard).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finishBoard = resolve;
      }),
  );
  vi.mocked(api.getWatchCatalog).mockImplementationOnce(
    () => new Promise(() => {}),
  );
  render(<BuyWatch {...props()} />);
  expect(api.getWatchCatalog).not.toHaveBeenCalled();
  finishBoard({ ...empty, rows: [structuredClone(row)] });
  const table = await screen.findByRole("table", { name: "观察公司对比" });
  await waitFor(() => expect(api.getWatchCatalog).toHaveBeenCalledOnce());
  expect(api.getWatchDetail).not.toHaveBeenCalled();
  expect(api.getWatchCatalogDetail).not.toHaveBeenCalled();
  fireEvent.click(within(table).getByRole("button", { name: /^测试公司/ }));
  await waitFor(() => expect(api.getWatchDetail).toHaveBeenCalledOnce());
  expect(api.getWatchDetail).toHaveBeenCalledWith(
    row.entry.symbol,
    expect.any(AbortSignal),
  );
});
