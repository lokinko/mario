import { expect, it } from "vitest";
import {
  formatAmount,
  formatMoney,
  formatNumber,
  formatPercent,
  formatSigned,
} from "./format";
it("preserves long decimal source strings and rounds only for display", () => {
  expect(formatMoney("999999999999999.99")).toBe("¥999,999,999,999,999.99");
  expect(formatMoney("1000000000000.005")).toBe("¥1,000,000,000,000.01");
  expect(formatMoney("-0.001")).toBe("¥0.00");
  expect(formatNumber("999999999999999.99995", 4)).toBe(
    "1,000,000,000,000,000",
  );
});
it("pads money with currency precision, groups quantities and retains requested price precision", () => {
  expect(formatMoney(1234)).toBe("¥1,234.00");
  expect(formatMoney("1234.5")).toBe("¥1,234.50");
  expect(formatMoney(1234.5, "JPY")).toBe("JP¥1,235");
  expect(formatAmount("1234.50", "USD")).toBe("1,234.50");
  expect(formatMoney(1.2345, "invalid")).toBe("invalid 1.23");
  expect(formatNumber("1234.123456", 4)).toBe("1,234.1235");
  expect(formatNumber("100.10000000")).toBe("100.1");
  expect(formatNumber(0.1 + 0.2)).toBe("0.3");
  expect(formatNumber(-0.001)).toBe("0");
});
it("uses signs independent of color and leaves rounded zero neutral", () => {
  expect(formatSigned("-1234.005")).toBe("−1,234.01");
  expect(formatSigned("1234")).toBe("+1,234.00");
  expect(formatSigned("-0.001")).toBe("0.00");
  expect(formatPercent(12.34)).toBe("+12.34%");
  expect(formatPercent(-12)).toBe("−12.00%");
  expect(formatPercent(0)).toBe("0.00%");
  expect(formatSigned("-12", (v) => formatMoney(v, "USD"))).toBe("−US$12.00");
});
