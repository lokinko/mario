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
  await screen.findByText("1 USD = 7 CNY");
  await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
  fireEvent.click(
    within(screen.getByRole("article", { name: "USD 兑 CNY 汇率" })).getByText(
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
  await screen.findByText("1 USD = 8 CNY");
  expect(portfolioApi.setFxOverride).toHaveBeenCalledWith("USD", 8);
  await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
  vi.mocked(portfolioApi.setFxOverride).mockResolvedValue(book);
  fireEvent.click(screen.getByRole("button", { name: "恢复自动" }));
  await screen.findByText("1 USD = 7 CNY");
  expect(portfolioApi.setFxOverride).toHaveBeenLastCalledWith("USD", null);
  await waitFor(() => expect(onUpdate).toHaveBeenCalledTimes(2));
});
it("shows involved rates directly with optional editors and other currencies", async () => {
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
  await screen.findByText("1 HKD = 0.91 CNY");
  expect(
    screen.getByRole("article", { name: "HKD 兑 CNY 汇率" }).closest("details"),
  ).toBeNull();
  expect(
    screen.getByRole("article", { name: "USD 兑 CNY 汇率" }).closest("details")
      ?.open,
  ).toBe(false);
  fireEvent.click(
    within(screen.getByRole("article", { name: "HKD 兑 CNY 汇率" })).getByText(
      "修改汇率",
    ),
  );
  expect(
    container.querySelector(".exchange-rate-row strong")?.textContent,
  ).toBe("1 HKD = 0.91 CNY");
  expect(screen.getAllByText("1 HKD = 0.91 CNY")).toHaveLength(1);
  expect(
    screen
      .getByRole("spinbutton", { name: "修改 HKD 汇率" })
      .getAttribute("placeholder"),
  ).toBe("0.91");
  expect(portfolioApi.getSnapshot).not.toHaveBeenCalled();
  const input = screen.getByRole("spinbutton", { name: "修改 HKD 汇率" });
  fireEvent.change(input, { target: { value: "0.91234567" } });
  vi.mocked(portfolioApi.setFxOverride).mockResolvedValue(book);
  fireEvent.click(screen.getAllByRole("button", { name: "保存修改" })[0]);
  await waitFor(() =>
    expect(portfolioApi.setFxOverride).toHaveBeenCalledWith("HKD", 0.91234567),
  );
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
