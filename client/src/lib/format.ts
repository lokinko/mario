const decimalFormatter = new Intl.NumberFormat("zh-CN", {
  useGrouping: false,
  maximumFractionDigits: 2,
});

export function formatNumber(value: number | string) {
  const formatted = decimalFormatter.format(Number(value));
  return formatted === "-0" ? "0" : formatted;
}

export function formatMoney(
  value: number,
  currency = "CNY",
  fractionDigits = 2,
) {
  fractionDigits = Math.min(2, Math.max(0, Math.trunc(fractionDigits)));
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
