import { useEffect, useState, type FormEvent } from "react";
import { ArrowUpRight, Search, Share2 } from "lucide-react";
import { dividendBriefs } from "./briefs";
import { DividendShare } from "./DividendShare";
import {
  dividendReturns,
  priceCapturedOn,
  priceOn,
  returnMethod,
  yieldMethod,
} from "./returns";
import { dividendPreviewHref, dividendQuestionHref } from "./route";
import snapshot from "./snapshot.json";
import "./dividend-preview.css";

const featured = ["600900", "600941", "601088"];

function findStock(query: string) {
  const term = query.trim();
  return snapshot.stocks.find(
    ({ symbol, name }) => symbol === term || name === term,
  );
}

export function DividendPreview() {
  const requested = new URLSearchParams(window.location.search).get("symbol");
  const [stock, setStock] = useState(() => findStock(requested ?? featured[0]));
  const [query, setQuery] = useState(requested ?? "");
  const [sharing, setSharing] = useState(false);

  useEffect(() => {
    const title = document.title;
    document.title = "红利观察 · mario";
    return () => {
      document.title = title;
    };
  }, []);

  function lookup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const result = findStock(query);
    setStock(result);
    window.history.replaceState(null, "", dividendPreviewHref(result?.symbol));
  }

  const latest = stock?.events[0];
  const previous = stock?.events[1];
  const brief = stock && dividendBriefs[stock.symbol];
  const market = stock && dividendReturns(stock.symbol);

  return (
    <div className="dividend-preview">
      <header className="dividend-preview-nav">
        <a className="dividend-preview-brand" href={dividendPreviewHref()}>
          <img
            src={`${import.meta.env.BASE_URL}mario-mark.svg`}
            alt=""
            width="29"
            height="29"
          />
          <strong>mario</strong>
          <span>红利观察</span>
        </a>
        <a
          className="dividend-preview-workspace"
          href={import.meta.env.BASE_URL}
        >
          进入工作台 <ArrowUpRight size={15} aria-hidden="true" />
        </a>
      </header>
      <main className="dividend-preview-main">
        <section
          className="dividend-preview-intro"
          aria-labelledby="dividend-preview-title"
        >
          <h1 id="dividend-preview-title">查一查这家公司的分红</h1>
          <p>核对每 10 股派了多少、哪天除息，再看 AI 如何解读历次变化。</p>
          <form className="dividend-preview-search" onSubmit={lookup}>
            <label htmlFor="dividend-stock">公司名称或 A 股代码</label>
            <div>
              <input
                id="dividend-stock"
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="例如 600900"
                autoComplete="off"
                maxLength={32}
                required
              />
              <button type="submit">
                <Search size={17} aria-hidden="true" /> 查询
              </button>
            </div>
          </form>
          <div className="dividend-preview-examples" aria-label="可查看的示例">
            {featured.map((symbol) => (
              <a
                key={symbol}
                href={dividendPreviewHref(symbol)}
                aria-current={stock?.symbol === symbol ? "true" : undefined}
              >
                {findStock(symbol)?.name}
              </a>
            ))}
          </div>
          <p className="dividend-preview-coverage">
            历史样本覆盖 {snapshot.stocks.length} 只 A 股，数据截至{" "}
            <time dateTime={snapshot.asOf}>{snapshot.asOf}</time>。
          </p>
        </section>
        <section
          className="dividend-preview-result"
          aria-label="分红查询结果"
          aria-live="polite"
        >
          {!stock && (
            <div className="dividend-preview-empty">
              <h2>样本中没有找到这家公司</h2>
              <p>请核对公司全名或股票代码，也可以点选上方的示例。</p>
            </div>
          )}
          {stock && latest && (
            <article
              className="dividend-ticket"
              aria-labelledby="dividend-stock-name"
            >
              <div className="dividend-ticket-summary">
                <div className="dividend-ticket-security">
                  <h2 id="dividend-stock-name">{stock.name}</h2>
                  <span>A 股 {stock.symbol}</span>
                </div>
                <p className="dividend-ticket-caption">
                  最近一次已实施现金分红
                </p>
                <div className="dividend-ticket-amount">
                  <strong>{latest.cashPerTen}</strong>
                  <span>元 / 10 股（税前）</span>
                </div>
                <dl className="dividend-ticket-events">
                  <div>
                    <dt>除息日</dt>
                    <dd>
                      <time dateTime={latest.exDate}>{latest.exDate}</time>
                    </dd>
                  </div>
                  {previous && (
                    <div>
                      <dt>上一次</dt>
                      <dd>
                        <time dateTime={previous.exDate}>
                          {previous.exDate}
                        </time>{" "}
                        <span>{previous.cashPerTen} 元 / 10 股</span>
                      </dd>
                    </div>
                  )}
                </dl>
              </div>
              {market && (
                <section
                  className="dividend-performance"
                  aria-label="股息率与历史涨跌"
                >
                  <dl className="dividend-performance-yield">
                    <div>
                      <dt>近 365 天税前股息率</dt>
                      <dd>
                        <strong>{market.yieldPct}</strong>
                      </dd>
                    </div>
                    <div>
                      <dt>每股派息</dt>
                      <dd>{market.dividendPerShare} 元</dd>
                    </div>
                    <div>
                      <dt>不复权收盘价</dt>
                      <dd>{market.close} 元</dd>
                    </div>
                  </dl>
                  <div className="dividend-performance-heading">
                    <h3>历史涨跌（前复权）</h3>
                    <span>
                      截至 <time dateTime={priceOn}>{priceOn}</time>
                    </span>
                  </div>
                  <dl className="dividend-performance-periods">
                    {market.periods.map(({ label, value }) => (
                      <div key={label}>
                        <dt>{label}</dt>
                        <dd>{value}</dd>
                      </div>
                    ))}
                  </dl>
                </section>
              )}
              <div className="dividend-ticket-actions">
                <details>
                  <summary>查看数据来源</summary>
                  <p>
                    样本截至{" "}
                    <time dateTime={snapshot.asOf}>{snapshot.asOf}</time>
                    。最近一次对应报告期截至{" "}
                    <time dateTime={latest.reportDate}>
                      {latest.reportDate}
                    </time>
                    。金额为单次税前派息，未计入分红预案。
                  </p>
                  {market && (
                    <>
                      <p>
                        {yieldMethod}
                        {returnMethod}
                      </p>
                      <p>
                        行情采集于{" "}
                        <time dateTime={priceCapturedOn}>
                          {priceCapturedOn}
                        </time>
                        。
                      </p>
                      <ul>
                        {market.periods.map(({ label, from }) => (
                          <li key={label}>
                            {label}{" "}
                            {from ? `${from} 至 ${priceOn}` : "历史不足"}
                          </li>
                        ))}
                      </ul>
                    </>
                  )}
                  <div className="dividend-source-links">
                    <a href={stock.source} target="_blank" rel="noreferrer">
                      东方财富分红数据{" "}
                      <ArrowUpRight size={14} aria-hidden="true" />
                    </a>
                    {market && (
                      <>
                        <a
                          href={market.rawSource}
                          target="_blank"
                          rel="noreferrer"
                        >
                          不复权日线{" "}
                          <ArrowUpRight size={14} aria-hidden="true" />
                        </a>
                        <a
                          href={market.adjustedSource}
                          target="_blank"
                          rel="noreferrer"
                        >
                          前复权日线{" "}
                          <ArrowUpRight size={14} aria-hidden="true" />
                        </a>
                      </>
                    )}
                  </div>
                </details>
                <button
                  type="button"
                  className="dividend-share-open"
                  onClick={() => setSharing(true)}
                >
                  <Share2 size={17} aria-hidden="true" /> 分享卡片
                </button>
              </div>
              {brief && (
                <section
                  className="dividend-brief"
                  aria-labelledby="dividend-brief-title"
                >
                  <p className="dividend-brief-label">AI 解读</p>
                  <h3 id="dividend-brief-title">{brief.headline}</h3>
                  <p>{brief.reading}</p>
                  <div className="dividend-brief-follow-up">
                    <h4>接下来查什么</h4>
                    <p>{brief.followUp}</p>
                  </div>
                  <p className="dividend-brief-note">
                    基于截至 {snapshot.asOf} 的分红样本预生成，供研究参考。
                  </p>
                  <a
                    className="dividend-brief-link"
                    href={dividendQuestionHref(stock.symbol)}
                  >
                    到工作台继续问 AI{" "}
                    <ArrowUpRight size={15} aria-hidden="true" />
                  </a>
                </section>
              )}
            </article>
          )}
        </section>
      </main>
      <footer className="dividend-preview-footer">
        分红记录仅供研究参考。
      </footer>
      {sharing && stock && (
        <DividendShare
          key={stock.symbol}
          stock={stock}
          onDismiss={() => setSharing(false)}
        />
      )}
    </div>
  );
}
