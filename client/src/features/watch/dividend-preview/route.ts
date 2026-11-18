type PreviewLocation = Pick<Location, "origin" | "pathname" | "search">;

export function isDividendPreview(location: PreviewLocation) {
  return (
    location.pathname === import.meta.env.BASE_URL &&
    new URLSearchParams(location.search).get("view") === "dividends"
  );
}

export function dividendPreviewHref(symbol?: string) {
  const query = new URLSearchParams({ view: "dividends" });
  if (symbol) query.set("symbol", symbol);
  return `${import.meta.env.BASE_URL}?${query}`;
}

export const dividendQuestionParam = "dividend";

export function dividendQuestionHref(symbol: string) {
  const query = new URLSearchParams({ [dividendQuestionParam]: symbol });
  return `${import.meta.env.BASE_URL}?${query}`;
}

export function dividendQuestionSymbol(search: string) {
  const symbol = new URLSearchParams(search).get(dividendQuestionParam);
  return symbol?.length === 6 && /^\d{6}$/.test(symbol) ? symbol : undefined;
}

export function dividendShareUrl(
  symbol: string,
  origin = window.location.origin,
) {
  return new URL(dividendPreviewHref(symbol), origin).href;
}
