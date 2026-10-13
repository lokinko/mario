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
import { ExchangeRates } from "./ExchangeRates";
import * as portfolioApi from "../../api/portfolio";
import fixture from "../../test/emptySnapshot.json";
import type { FxBook, Snapshot } from "../../types";
vi.mock("../../api/portfolio", () => ({
  getFxSettings: vi.fn(),
  getSnapshot: vi.fn(),
  refreshFxSettings: vi.fn(),
  setFxOverride: vi.fn(),
}));
const book: FxBook = {
  baseCurrency: "CNY",
  checkedAt: 1791676800,
  requestedOn: "2026-10-11",
  quotes: [
    {
      fromCurrency: "USD",
      toCurrency: "CNY",
      rate: 7,
      requestedOn: "2026-10-11",
      observedOn: "2026-10-09",
    } as FxBook["quotes"][number],
  ],
  manual: {},
  error: "",
};
beforeEach(() => {
  localStorage.clear();
  vi.mocked(portfolioApi.getFxSettings).mockResolvedValue(
    structuredClone(book),
  );
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue(fixture as Snapshot);
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
async function open() {
  const onUpdate = vi.fn();
  render(<ExchangeRates baseCurrency="CNY" onUpdate={onUpdate} />);
  await screen.findByLabelText("1 USD = 7 CNY");
  await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
  fireEvent.click(screen.getByText("管理汇率"));
  fireEvent.click(
    within(screen.getByRole("article", { name: "USD 汇率管理" })).getByText(
      "修改汇率",
    ),
  );
  return onUpdate;
}
it("shows automatically loaded values and saves a persistent manual rate then restores automatic", async () => {
  const onUpdate = await open();
  expect(
    screen.getByText(/沿用最近工作日/, { selector: "small" }),
  ).toBeTruthy();
  fireEvent.change(screen.getByRole("spinbutton", { name: "修改 USD 汇率" }), {
    target: { value: "8" },
  });
  vi.mocked(portfolioApi.setFxOverride).mockResolvedValue({
    ...book,
    manual: { USD: { rate: 8, observedOn: "2026-10-11" } },
  });
  fireEvent.click(screen.getAllByRole("button", { name: "保存修改" })[0]);
  await screen.findByLabelText("1 USD = 8 CNY");
  expect(portfolioApi.setFxOverride).toHaveBeenCalledWith("USD", 8);
  await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
  vi.mocked(portfolioApi.setFxOverride).mockResolvedValue(book);
  fireEvent.click(screen.getByRole("button", { name: "恢复自动" }));
  await screen.findByLabelText("1 USD = 7 CNY");
  expect(portfolioApi.setFxOverride).toHaveBeenLastCalledWith("USD", null);
  await waitFor(() => expect(onUpdate).toHaveBeenCalledTimes(2));
});
it("shows involved rates directly and lets users select other currencies", async () => {
  vi.mocked(portfolioApi.getFxSettings).mockResolvedValue({
    ...book,
    quotes: [
      ...book.quotes,
      { ...book.quotes[0], fromCurrency: "HKD", rate: 0.91234567 },
    ],
  });
  const { container } = render(
    <ExchangeRates
      baseCurrency="CNY"
      usedCurrencies={["HKD", "CNY", "HKD"]}
      onUpdate={vi.fn()}
    />,
  );
  await screen.findByLabelText("1 HKD = 0.91 CNY");
  expect(
    screen.getByRole("article", { name: "HKD 兑 CNY 汇率" }).closest("details"),
  ).toBeNull();
  expect(screen.queryByRole("article", { name: "USD 兑 CNY 汇率" })).toBeNull();
  fireEvent.click(screen.getByRole("checkbox", { name: "USD" }));
  expect(screen.getByRole("article", { name: "USD 兑 CNY 汇率" })).toBeTruthy();
  fireEvent.click(screen.getByText("管理汇率"));
  fireEvent.click(
    within(screen.getByRole("article", { name: "HKD 汇率管理" })).getByText(
      "修改汇率",
    ),
  );
  expect(
    container.querySelector(".exchange-rate-card:nth-child(2) strong")
      ?.textContent,
  ).toBe("0.91");
  expect(screen.getAllByLabelText("1 HKD = 0.91 CNY")).toHaveLength(1);
  expect(
    screen
      .getByRole("spinbutton", { name: "修改 HKD 汇率" })
      .getAttribute("placeholder"),
  ).toBe("0.91");
  expect(portfolioApi.getSnapshot).not.toHaveBeenCalled();
  const input = screen.getByRole("spinbutton", { name: "修改 HKD 汇率" });
  fireEvent.change(input, { target: { value: "0.91234567" } });
  vi.mocked(portfolioApi.setFxOverride).mockResolvedValue(book);
  fireEvent.click(
    within(screen.getByRole("article", { name: "HKD 汇率管理" })).getByRole(
      "button",
      { name: "保存修改" },
    ),
  );
  await waitFor(() =>
    expect(portfolioApi.setFxOverride).toHaveBeenCalledWith("HKD", 0.91234567),
  );
});
it("remembers deselected currencies across mounts without changing rates or assets", async () => {
  const props = {
    baseCurrency: "CNY",
    usedCurrencies: ["USD"],
    onUpdate: vi.fn(),
  };
  const first = render(<ExchangeRates {...props} />);
  await screen.findByLabelText("1 USD = 7 CNY");
  fireEvent.click(screen.getByRole("checkbox", { name: "USD" }));
  expect(screen.queryByRole("article", { name: "USD 兑 CNY 汇率" })).toBeNull();
  expect(screen.getByText("勾选币种后显示汇率。")).toBeTruthy();
  first.unmount();
  render(<ExchangeRates {...props} />);
  await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
  expect(
    (screen.getByRole("checkbox", { name: "USD" }) as HTMLInputElement).checked,
  ).toBe(false);
  expect(screen.queryByRole("article", { name: "USD 兑 CNY 汇率" })).toBeNull();
  fireEvent.click(screen.getByRole("checkbox", { name: "JPY" }));
  expect(
    screen.getByRole("article", { name: "JPY 兑 CNY 汇率" }).textContent,
  ).toContain("待更新");
  expect(portfolioApi.setFxOverride).not.toHaveBeenCalled();
  expect(portfolioApi.refreshFxSettings).not.toHaveBeenCalled();
  expect(portfolioApi.getSnapshot).not.toHaveBeenCalled();
  expect(props.onUpdate).not.toHaveBeenCalled();
});
it("ignores malformed preferences and excludes the new base currency", async () => {
  localStorage.setItem("mario.exchange.visible-currencies", "invalid json");
  const { rerender } = render(
    <ExchangeRates baseCurrency="CNY" onUpdate={vi.fn()} />,
  );
  await screen.findByLabelText("1 USD = 7 CNY");
  vi.mocked(portfolioApi.getFxSettings).mockResolvedValue({
    ...book,
    baseCurrency: "USD",
    quotes: [
      { ...book.quotes[0], fromCurrency: "CNY", toCurrency: "USD", rate: 0.14 },
    ],
  });
  rerender(<ExchangeRates baseCurrency="USD" onUpdate={vi.fn()} />);
  await screen.findByLabelText("1 CNY = 0.14 USD");
  expect(screen.queryByRole("checkbox", { name: "USD" })).toBeNull();
  expect(screen.queryByLabelText("1 USD = 7 CNY")).toBeNull();
});
it("rejects empty or nonpositive overrides and retains the draft after a failed save", async () => {
  await open();
  const input = screen.getByRole("spinbutton", {
    name: "修改 USD 汇率",
  }) as HTMLInputElement;
  const save = screen.getAllByRole("button", {
    name: "保存修改",
  })[0] as HTMLButtonElement;
  fireEvent.change(input, { target: { value: "0" } });
  expect(save.disabled).toBe(true);
  fireEvent.change(input, { target: { value: "8.2" } });
  vi.mocked(portfolioApi.setFxOverride).mockRejectedValue(
    new Error("网络中断"),
  );
  fireEvent.click(save);
  expect((await screen.findByRole("alert")).textContent).toContain("网络中断");
  expect(input.value).toBe("8.2");
});
