const decimalPattern = /^-?\d+(?:\.\d+)?$/;

// Round decimal source strings without passing through binary floating point.
function decimalDisplay(value: string, digits: number, fixed = false) {
  const negative = value.startsWith("-");
  const [whole, fraction = ""] = value.replace(/^-/, "").split(".");
  const kept = fraction.slice(0, digits).padEnd(digits, "0");
  let units = BigInt(whole) * 10n ** BigInt(digits) + BigInt(kept || "0");
  if (Number(fraction[digits] ?? "0") >= 5) units += 1n;
  const raw = units.toString().padStart(digits + 1, "0");
  const integer = (digits ? raw.slice(0, -digits) : raw).replace(
    /\B(?=(\d{3})+(?!\d))/g,
    ",",
  );
  const decimals = digits
    ? fixed
      ? raw.slice(-digits)
      : raw.slice(-digits).replace(/0+$/, "")
    : "";
  return {
    negative: negative && units !== 0n,
    text: integer + (decimals ? "." + decimals : ""),
  };
}

export function formatNumber(
  value: number | string,
  fractionDigits = 2,
  fixed = false,
) {
  const digits = Math.min(20, Math.max(0, Math.trunc(fractionDigits)));
  if (typeof value === "string" && decimalPattern.test(value)) {
    const amount = decimalDisplay(value, digits, fixed);
    return (amount.negative ? "−" : "") + amount.text;
  }
  const number = Number(value);
  const rounded = Number(number.toFixed(digits));
  return new Intl.NumberFormat("zh-CN", {
    useGrouping: true,
    minimumFractionDigits: fixed ? digits : 0,
    maximumFractionDigits: digits,
  })
    .format(rounded === 0 ? 0 : rounded)
    .replace(/-/g, "−");
}

export function currencyDigits(currency = "CNY") {
  try {
    return (
      new Intl.NumberFormat("zh-CN", {
        style: "currency",
        currency,
      }).resolvedOptions().maximumFractionDigits ?? 2
    );
  } catch {
    return 2;
  }
}

export function formatAmount(value: number | string, currency = "CNY") {
  return formatNumber(value, currencyDigits(currency), true);
}

export function formatMoney(
  value: number | string,
  currency = "CNY",
  fractionDigits = currencyDigits(currency),
) {
  const digits = Math.min(
    currencyDigits(currency),
    Math.max(0, Math.trunc(fractionDigits)),
  );
  const formatted = formatNumber(value, digits, true);
  const negative = formatted.startsWith("−");
  const unsigned = formatted.replace(/^−/, "");
  try {
    const parts = new Intl.NumberFormat("zh-CN", {
      style: "currency",
      currency,
      minimumFractionDigits: 0,
      maximumFractionDigits: 0,
    }).formatToParts(0);
    return (
      (negative ? "−" : "") +
      parts
        .map((part) => (part.type === "integer" ? unsigned : part.value))
        .join("")
    );
  } catch {
    return `${currency} ${formatted}`;
  }
}

export function formatSigned(
  value: number | string,
  formatter: (value: number | string) => string = (value) =>
    formatNumber(value, 2, true),
) {
  const formatted = formatter(value).replace(/-/g, "−");
  const zero = !/[1-9]/.test(formatted.replace(/[^0-9]/g, ""));
  return !zero && !String(value).startsWith("-") ? "+" + formatted : formatted;
}

export function formatPercent(value: number | string) {
  return formatSigned(value) + "%";
}
