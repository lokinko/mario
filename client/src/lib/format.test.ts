import { expect, it } from "vitest";
import { formatMoney, formatNumber } from "./format";

it("preserves decimal source precision beyond the float unit and rounds only for display", () => {
  expect(formatMoney("999999999999999.99")).toBe("¥999,999,999,999,999.99");
  expect(formatMoney("1000000000000.005")).toBe("¥1,000,000,000,000.01");
  expect(formatNumber("-0.005")).toBe("-0.01");
  expect(formatMoney("-0.001")).toBe("¥0");
});

it("shows at most two decimals without padding or floating point tails", () => {
  expect(formatNumber(12.345678)).toBe("12.35");
  expect(formatNumber("100.10000000")).toBe("100.1");
  expect(formatNumber(0.1 + 0.2)).toBe("0.3");
  expect(formatNumber(-0.001)).toBe("0");
  expect(formatMoney(1234.5678)).toBe("¥1,234.57");
  expect(formatMoney(1234)).toBe("¥1,234");
  expect(formatMoney(1.2345, "USD", 8)).toBe("US$1.23");
  expect(formatMoney(-0.001)).toBe("¥0");
  expect(formatMoney(1.2345, "invalid")).toBe("invalid 1.23");
  expect(formatMoney(1.2345, "invalid", 0)).toBe("invalid 1");
});
