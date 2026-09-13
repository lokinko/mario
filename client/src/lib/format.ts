export function formatMoney(value: number, currency = "CNY") {
  if (Object.is(value, -0)) value = 0;
  try {
    return new Intl.NumberFormat("zh-CN", {
      style: "currency",
      currency,
      maximumFractionDigits: 0,
    }).format(value);
  } catch {
    return `${currency} ${value.toFixed(0)}`;
  }
}
