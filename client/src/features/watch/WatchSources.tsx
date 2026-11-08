function observationLabel(url: string) {
  if (/SHAREBONUS/i.test(url)) return "已实施分红";
  if (/secid=1\.000001\b|\/chart\/000001\.SS\b/.test(url))
    return "交易日历（上证指数）";
  if (/[?&]fqt=2\b/.test(url)) return "证券日线（前复权）";
  if (/kline|\/finance\/chart\//.test(url)) return "证券日线";
  return "公开来源";
}

function host(url: string) {
  try {
    return new URL(url).hostname;
  } catch {
    return url;
  }
}

export function WatchSources({
  company = [],
  observation = [],
}: {
  company?: string[];
  observation?: string[];
}) {
  const items = [
    ...company.map((url, i) => ({
      url,
      label: i === 0 ? "交易所公司资料" : "公司公开报告",
    })),
    ...observation.map((url) => ({ url, label: observationLabel(url) })),
  ].filter(
    (item, i, all) => all.findIndex((other) => other.url === item.url) === i,
  );
  if (!items.length) return null;
  return (
    <details className="watch-sources">
      <summary>来源 {items.length}</summary>
      <ul>
        {items.map((item) => (
          <li key={item.url}>
            <span>{item.label}</span>
            <a href={item.url} target="_blank" rel="noreferrer">
              {host(item.url)} ↗
            </a>
          </li>
        ))}
      </ul>
    </details>
  );
}
