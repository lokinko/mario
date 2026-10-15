const decimalFormatter = new Intl.NumberFormat("zh-CN", {
  useGrouping: false,
  maximumFractionDigits: 2,
});

export function formatNumber(value: number | string) {
  if (typeof value === "string" && /^-?\d+(?:\.\d+)?$/.test(value)) {
    const amount = decimalDisplay(value, 2);
    return (amount.negative ? "-" : "") + amount.text;
  }
  const formatted = decimalFormatter.format(Number(value));
  return formatted === "-0" ? "0" : formatted;
}

export function formatMoney(
  value: number | string,
  currency = "CNY",
  fractionDigits = 2,
) {
  fractionDigits = Math.min(2, Math.max(0, Math.trunc(fractionDigits)));
  if (typeof value === "string" && /^-?\d+(?:\.\d+)?$/.test(value)) {
    const amount = decimalDisplay(value, fractionDigits);
    try {
      const parts = new Intl.NumberFormat("zh-CN", {
        style: "currency",
        currency,
        minimumFractionDigits: 0,
        maximumFractionDigits: fractionDigits,
      }).formatToParts(0);
      return (
        (amount.negative ? "-" : "") +
        parts
          .filter((part) => part.type !== "fraction" && part.type !== "decimal")
          .map((part) =>
            part.type === "integer"
              ? amount.text.replace(/\B(?=(\d{3})+(?!\d))/g, ",")
              : part.value,
          )
          .join("")
      );
    } catch {
      return `${currency} ${amount.negative ? "-" : ""}${amount.text}`;
    }
  }
  value = Number(value);
  // Normalize tiny negative values that round to zero in the display only.
  if (Number(value.toFixed(fractionDigits)) === 0) value = 0;
  try {
    return new Intl.NumberFormat("zh-CN", {
      style: "currency",
      currency,
      minimumFractionDigits: 0,
      maximumFractionDigits: fractionDigits,
    }).format(value);
  } catch {
    return `${currency} ${formatNumber(Number(value.toFixed(fractionDigits)))}`;
  }
}

// Decimal source strings never pass through binary floating point for display.
function decimalDisplay(value: string, digits: number) {
  const negative = value.startsWith("-");
  const [whole, fraction = ""] = value.replace(/^-/, "").split(".");
  const kept = fraction.slice(0, digits).padEnd(digits, "0");
  let units = BigInt(whole) * 10n ** BigInt(digits) + BigInt(kept || "0");
  if (Number(fraction[digits] ?? "0") >= 5) units += 1n;
  const raw = units.toString().padStart(digits + 1, "0");
  const integer = digits ? raw.slice(0, -digits) : raw;
  const decimals = digits ? raw.slice(-digits).replace(/0+$/, "") : "";
  return {
    negative: negative && units !== 0n,
    text: integer + (decimals ? "." + decimals : ""),
  };
}
