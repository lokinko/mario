// @vitest-environment jsdom
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { WatchSources } from "./WatchSources";

afterEach(cleanup);

it("collects company and observation sources behind one entry, labelled by content", () => {
  const kline =
    "https://push2his.eastmoney.com/api/qt/stock/kline/get?secid=1.600886&klt=101";
  render(
    <WatchSources
      company={["https://www.sse.com.cn/a", "https://www.sse.com.cn/b"]}
      observation={[
        `${kline}&fqt=0`,
        `${kline}&fqt=2`,
        "https://push2his.eastmoney.com/api/qt/stock/kline/get?secid=1.000001&fqt=0",
        "https://datacenter-web.eastmoney.com/api/data/v1/get?reportName=RPT_SHAREBONUS_DET",
        "https://datacenter-web.eastmoney.com/api/data/v1/get?reportName=RPT_SHAREBONUS_DET",
      ]}
    />,
  );
  expect(screen.getByText("来源 6")).toBeTruthy();
  const labels = within(screen.getByRole("list"))
    .getAllByRole("listitem")
    .map((item) => item.querySelector("span")?.textContent);
  expect(labels).toEqual([
    "交易所公司资料",
    "公司公开报告",
    "证券日线",
    "证券日线（前复权）",
    "交易日历（上证指数）",
    "已实施分红",
  ]);
});

it("renders nothing without sources", () => {
  const { container } = render(<WatchSources />);
  expect(container.innerHTML).toBe("");
});
