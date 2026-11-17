// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { AllocationChart } from "./AllocationChart";
import * as portfolioApi from "../../api/portfolio";
import type { Snapshot } from "../../types/user";

vi.mock("../../api/portfolio", () => ({ getAllocation: vi.fn() }));
afterEach(cleanup);

it("shows server-computed weights and switches between asset class and industry", async () => {
  vi.mocked(portfolioApi.getAllocation).mockResolvedValue({
    baseCurrency: "CNY",
    byClass: [
      { label: "股票", value: "600", pct: "60" },
      { label: "现金", value: "400", pct: "40" },
    ],
    byIndustry: [
      { label: "现金", value: "400", pct: "40" },
      { label: "银行", value: "350", pct: "35" },
      { label: "其他股票", value: "250", pct: "25" },
    ],
    missing: 1,
  });
  render(<AllocationChart snapshot={{} as Snapshot} />);
  const chart = await screen.findByRole("img");
  expect(chart.getAttribute("aria-label")).toBe("股票 60%，现金 40%");
  expect(screen.getByText("600.00")).toBeTruthy();
  expect(screen.getByText(/1 项缺少汇率或待处理/)).toBeTruthy();
  fireEvent.click(screen.getByText("行业"));
  expect(screen.getByRole("img").getAttribute("aria-label")).toBe(
    "现金 40%，银行 35%，其他股票 25%",
  );
  expect(screen.getByText(/目录外的归入“其他股票”/)).toBeTruthy();
  expect(portfolioApi.getAllocation).toHaveBeenCalledOnce();
});

it("stays hidden when nothing can be allocated", async () => {
  vi.mocked(portfolioApi.getAllocation).mockResolvedValue({
    baseCurrency: "CNY",
    byClass: [],
    byIndustry: [],
    missing: 0,
  });
  render(<AllocationChart snapshot={{} as Snapshot} />);
  await Promise.resolve();
  expect(screen.queryByRole("region", { name: "资产配置" })).toBeNull();
});
