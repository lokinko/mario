// @vitest-environment jsdom
import { defaultRules } from "../../test/watchRules";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WatchCatalogEditor } from "./WatchCatalogEditor";
import * as api from "../../api/watch";
import catalogFixture from "../../../../server/data/watch-catalog.json";

vi.mock("../../api/watch", () => ({
  saveWatchCatalog: vi.fn(),
  getWatchCatalog: vi.fn(),
  resolveCatalogCompany: vi.fn(),
}));
const company: api.CatalogCompany = {
  symbol: "600000.SS",
  name: "浦发银行",
  sources: [],
};
const board = (): api.CatalogBoard => ({
  catalog: structuredClone(catalogFixture),
  revision: 3,
  canEdit: true,
  rows: [],
  rules: defaultRules,
});
const props = () => ({
  board: board(),
  initialIndustry: "banks",
  onClose: vi.fn(),
  onEdited: vi.fn(),
  onSaved: vi.fn(),
});
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.resolveCatalogCompany).mockResolvedValue(company);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function apply(query = "浦发银行") {
  fireEvent.change(screen.getByLabelText("公司名称或证券代码"), {
    target: { value: query },
  });
  fireEvent.click(screen.getByRole("button", { name: "应用公司修改" }));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "保存公共目录" })).toHaveProperty(
      "disabled",
      false,
    ),
  );
}
it("looks up company identity without asking for public sources and saves pending evidence", async () => {
  const p = props();
  vi.mocked(api.saveWatchCatalog).mockResolvedValue({
    ...board(),
    revision: 4,
  });
  vi.spyOn(window, "confirm").mockReturnValue(true);
  render(<WatchCatalogEditor {...p} />);
  expect(screen.queryByLabelText(/公开来源/)).toBeNull();
  await apply();
  expect(api.resolveCatalogCompany).toHaveBeenCalledWith("浦发银行");
  expect(api.saveWatchCatalog).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "移除 工商银行" }));
  fireEvent.click(screen.getByRole("button", { name: "保存公共目录" }));
  await waitFor(() => expect(p.onSaved).toHaveBeenCalled());
  const [catalog, revision] = vi.mocked(api.saveWatchCatalog).mock.calls[0];
  expect(revision).toBe(3);
  expect(catalog.industries[0].companies.at(-1)).toEqual(company);
  expect(
    catalog.industries[0].companies.some((c) => c.symbol === "601398.SS"),
  ).toBe(false);
  expect(catalog.industries[1]).toEqual(catalogFixture.industries[1]);
});
it("rejects a duplicate resolved code and preserves input when identity lookup fails", async () => {
  vi.mocked(api.resolveCatalogCompany).mockResolvedValue(
    catalogFixture.industries[0].companies[0],
  );
  render(<WatchCatalogEditor {...props()} />);
  fireEvent.change(screen.getByLabelText("公司名称或证券代码"), {
    target: { value: "600036" },
  });
  fireEvent.click(screen.getByRole("button", { name: "应用公司修改" }));
  await screen.findByText("该公司已在公共目录中，请编辑原有记录");
  vi.mocked(api.resolveCatalogCompany).mockRejectedValue(
    new Error("公司身份暂未确认，请稍后重试"),
  );
  fireEvent.change(screen.getByLabelText("公司名称或证券代码"), {
    target: { value: "未知公司" },
  });
  fireEvent.click(screen.getByRole("button", { name: "应用公司修改" }));
  await screen.findByText(/公司身份暂未确认/);
  expect(screen.getByLabelText("公司名称或证券代码")).toHaveProperty(
    "value",
    "未知公司",
  );
  expect(api.saveWatchCatalog).not.toHaveBeenCalled();
});
it("keeps the draft and original revision when background source enrichment advances the board", async () => {
  const p = props();
  vi.mocked(api.saveWatchCatalog).mockRejectedValue(
    new Error("公共目录已被更新"),
  );
  const view = render(<WatchCatalogEditor {...p} />);
  await apply("600000");
  view.rerender(
    <WatchCatalogEditor {...p} board={{ ...board(), revision: 9 }} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "保存公共目录" }));
  await screen.findByText(/公共目录已被更新/);
  expect(vi.mocked(api.saveWatchCatalog).mock.calls[0][1]).toBe(3);
  expect(screen.getByText("浦发银行 · 600000.SS")).toBeTruthy();
  expect(p.onSaved).not.toHaveBeenCalled();
  vi.spyOn(window, "confirm").mockReturnValue(false);
  fireEvent.click(screen.getByRole("button", { name: "重新读取目录" }));
  expect(api.getWatchCatalog).not.toHaveBeenCalled();
  vi.mocked(window.confirm).mockReturnValue(true);
  vi.mocked(api.getWatchCatalog).mockResolvedValue({ ...board(), revision: 9 });
  fireEvent.click(screen.getByRole("button", { name: "重新读取目录" }));
  await waitFor(() =>
    expect(screen.queryByText("浦发银行 · 600000.SS")).toBeNull(),
  );
});
