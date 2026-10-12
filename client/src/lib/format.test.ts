import { expect, it } from "vitest";
import { formatMoney, formatNumber } from "./format";

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
