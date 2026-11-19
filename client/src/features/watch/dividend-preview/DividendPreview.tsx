import { useEffect, useState } from "react";
import { Button, Input } from "antd";
import { ArrowUpRight, Search, Share2 } from "lucide-react";
import { EmptyState } from "../../../components/EmptyState";
import { PageHeader } from "../../../components/PageHeader";
import { dividendCard } from "./card";
import { DividendShare } from "./DividendShare";
import { dividendPreviewHref, dividendQuestionHref } from "./route";
import snapshot from "./snapshot.json";
import "./dividend-preview.css";

const examples = ["600900", "601088", "000333"].flatMap((code) =>
  snapshot.stocks.filter(({ symbol }) => symbol === code),
);

function findCard(query: string) {
  const term = query.trim();
  const stock = snapshot.stocks.find(
    ({ symbol, name }) => symbol === term || name === term,
  );
  return stock && dividendCard(stock);
}

export function DividendPreview() {
  const requested = new URLSearchParams(window.location.search).get("symbol");
  const [card, setCard] = useState(() => findCard(requested ?? "600900"));
  const [query, setQuery] = useState(requested ?? "");
  const [sharing, setSharing] = useState(false);

  useEffect(() => {
    const title = document.title;
    document.title = "红利观察 · mario";
    return () => {
      document.title = title;
    };
  }, []);

  function lookup(term: string) {
    const result = findCard(term);
    setCard(result);
    window.history.replaceState(null, "", dividendPreviewHref(result?.symbol));
  }

  return (
    <div className="dividend-preview">
      <header className="workspace-bar dividend-preview-nav">
        <a
          className="workspace-location dividend-preview-brand"
          href={dividendPreviewHref()}
        >
          <img
            className="brand-mark"
            src={`${import.meta.env.BASE_URL}mario-mark.svg`}
            alt=""
            width="32"
            height="32"
          />
          <strong>mario</strong>
          <span>红利观察</span>
        </a>
        <Button
          type="text"
          href={import.meta.env.BASE_URL}
          icon={<ArrowUpRight size={15} aria-hidden="true" />}
        >
          进入工作台
        </Button>
      </header>
      <main className="page narrow dividend-preview-main">
        <PageHeader title="查公司分红" />
        <div className="panel dividend-preview-query">
          <form
            className="dividend-preview-search"
            onSubmit={(event) => {
              event.preventDefault();
              lookup(query);
            }}
          >
            <label htmlFor="dividend-stock">公司名称或 A 股代码</label>
            <div>
              <Input
                id="dividend-stock"
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="例如 600900"
                autoComplete="off"
                maxLength={32}
                required
              />
              <Button
                type="primary"
                htmlType="submit"
                icon={<Search size={16} aria-hidden="true" />}
              >
                查询
              </Button>
            </div>
          </form>
          <div
            className="dividend-preview-examples"
            role="group"
            aria-label="示例公司"
          >
            <span aria-hidden="true">试查</span>
            {examples.map(({ symbol, name }) => (
              <button
                className="text-button"
                key={symbol}
                type="button"
                onClick={() => {
                  setQuery(symbol);
                  lookup(symbol);
                }}
              >
                {name}
              </button>
            ))}
          </div>
        </div>
        <section
          className="dividend-preview-result"
          aria-label="分红查询结果"
          aria-live="polite"
        >
          {!card && (
            <div className="panel">
              <EmptyState title="样本中没有找到这家公司">
                请核对公司全名或 A 股代码。
              </EmptyState>
            </div>
          )}
          {card && (
            <article
              className="panel dividend-ticket"
              aria-labelledby="dividend-stock-name"
            >
              <header className="dividend-ticket-security">
                <h2 id="dividend-stock-name">{card.name}</h2>
                <span>A 股 {card.symbol}</span>
              </header>
              <div className="dividend-brief">
                {card.sections.map((section) => (
                  <section
                    key={section.kind}
                    data-kind={section.kind}
                    aria-labelledby={`dividend-${section.kind}-title`}
                  >
                    <h3 id={`dividend-${section.kind}-title`}>
                      {section.title}
                    </h3>
                    <p>{section.text}</p>
                    {section.kind === "performance" && (
                      <dl
                        className="dividend-ticket-metrics"
                        aria-label="分红与股价数据"
                      >
                        {section.metrics.map(({ label, value, note, date }) => (
                          <div key={label}>
                            <dt>{label}</dt>
                            <dd>
                              <strong>{value}</strong>
                              <span className="dividend-metric-note">
                                {note}
                              </span>
                              {date && (
                                <span className="dividend-metric-note">
                                  {date.label}{" "}
                                  <time dateTime={date.value}>
                                    {date.value}
                                  </time>
                                </span>
                              )}
                            </dd>
                          </div>
                        ))}
                      </dl>
                    )}
                  </section>
                ))}
              </div>
              <div className="dividend-ticket-actions">
                {card.sections.length > 1 && (
                  <a
                    className="dividend-brief-link"
                    href={dividendQuestionHref(card.symbol)}
                  >
                    到工作台继续问 AI{" "}
                    <ArrowUpRight size={15} aria-hidden="true" />
                  </a>
                )}
                <Button
                  icon={<Share2 size={16} aria-hidden="true" />}
                  onClick={() => setSharing(true)}
                >
                  分享卡片
                </Button>
              </div>
            </article>
          )}
        </section>
      </main>
      {sharing && card && (
        <DividendShare
          key={card.symbol}
          card={card}
          onDismiss={() => setSharing(false)}
        />
      )}
    </div>
  );
}
