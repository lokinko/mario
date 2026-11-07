// @vitest-environment jsdom
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
}));
const board = (): api.CatalogBoard => ({
  catalog: structuredClone(catalogFixture),
  revision: 3,
  canEdit: true,
  rows: [],
  minYieldPct: 5,
  bandDistancePct: 2,
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
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
function fillCompany(
  symbol = "600000.SH",
  name = "浦发银行",
  sources = "https://example.test/exchange\nhttps://example.test/report",
) {
  fireEvent.change(screen.getByLabelText("证券代码"), {
    target: { value: symbol },
  });
  fireEvent.change(screen.getByLabelText("公司名称"), {
    target: { value: name },
  });
  fireEvent.change(screen.getByLabelText("公开来源（每行一个，至少两个）"), {
    target: { value: sources },
  });
}
it("adds, edits and removes companies only after explicit shared-directory save", async () => {
  const p = props();
  vi.mocked(api.saveWatchCatalog).mockResolvedValue({
    ...board(),
    revision: 4,
  });
  vi.spyOn(window, "confirm").mockReturnValue(true);
  render(<WatchCatalogEditor {...p} />);
  fillCompany();
  expect(
    (screen.getByRole("button", { name: "保存公共目录" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "应用公司修改" }));
  expect(api.saveWatchCatalog).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "编辑 招商银行" }));
  fireEvent.change(screen.getByLabelText("公司名称"), {
    target: { value: "招商银行（已核查）" },
  });
  fireEvent.click(screen.getByRole("button", { name: "应用公司修改" }));
  fireEvent.click(screen.getByRole("button", { name: "移除 工商银行" }));
  fireEvent.click(screen.getByRole("button", { name: "保存公共目录" }));
  await waitFor(() => expect(p.onSaved).toHaveBeenCalled());
  const [catalog, revision] = vi.mocked(api.saveWatchCatalog).mock.calls[0];
  expect(revision).toBe(3);
  expect(catalog.industries[0].companies.map((c) => c.name)).toEqual([
    "农业银行",
    "浦发银行",
    "招商银行（已核查）",
  ]);
  expect(catalog.industries[0].companies[1].symbol).toBe("600000.SS");
  expect(catalog.industries[1]).toEqual(catalogFixture.industries[1]);
});
it("rejects duplicates and missing source evidence without a request", () => {
  render(<WatchCatalogEditor {...props()} />);
  fillCompany("600036.SH");
  fireEvent.click(screen.getByRole("button", { name: "应用公司修改" }));
  expect(screen.getByText("该公司已在公共目录中，请编辑原有记录")).toBeTruthy();
  fillCompany("600000.SH", "浦发银行", "https://example.test/exchange");
  fireEvent.click(screen.getByRole("button", { name: "应用公司修改" }));
  expect(screen.getByText(/请至少填写两个 HTTPS 来源/)).toBeTruthy();
  expect(api.saveWatchCatalog).not.toHaveBeenCalled();
});
it("keeps the draft and its original revision when background reads advance the parent board", async () => {
  const p = props();
  vi.mocked(api.saveWatchCatalog).mockRejectedValue(
    new Error("公共目录已被更新"),
  );
  const view = render(<WatchCatalogEditor {...p} />);
  fillCompany();
  fireEvent.click(screen.getByRole("button", { name: "应用公司修改" }));
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
