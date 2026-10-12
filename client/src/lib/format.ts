export function formatMoney(
  value: number,
  currency = "CNY",
  fractionDigits = 0,
) {
  if (Object.is(value, -0)) value = 0;
  try {
    return new Intl.NumberFormat("zh-CN", {
      style: "currency",
      currency,
      minimumFractionDigits: fractionDigits,
      maximumFractionDigits: fractionDigits,
    }).format(value);
  } catch {
    return `${currency} ${value.toFixed(fractionDigits)}`;
  }
}
