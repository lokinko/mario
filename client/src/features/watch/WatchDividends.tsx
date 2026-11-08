import type { Observation } from "../../api/watch";

export function WatchDividends({ latest }: { latest: Observation | null }) {
  const history = latest?.dividendHistory;
  const events = history?.events ?? latest?.dividendEvents ?? [];
  const covered =
    history &&
    latest?.dividendBasis &&
    latest.dividendBasis.asOf >= history.end;
  return (
    <section className="watch-detail-section">
      <h3>{history ? "近三年已实施分红" : "TTM 已实施分红"}</h3>
      {events.length ? (
        <div className="table-scroll">
          <table
            className="watch-events"
            aria-label={
              history ? "近三年已实施分红事件" : "TTM 窗口已实施分红事件"
            }
          >
            <thead>
              <tr>
                <th>除息日</th>
                <th>报告期</th>
                <th>每十股派现 / 元</th>
                <th>每十股送股</th>
                <th>每十股转增</th>
                <th>TTM 窗口</th>
              </tr>
            </thead>
            <tbody>
              {[...events]
                .sort(
                  (a, b) =>
                    b.exDate.localeCompare(a.exDate) ||
                    (b.reportDate ?? "").localeCompare(a.reportDate ?? ""),
                )
                .map((event, i) => (
                  <tr key={`${event.exDate}-${event.reportDate ?? i}`}>
                    <td>除息日 {event.exDate}</td>
                    <td>
                      {event.reportDate ? `报告期 ${event.reportDate}` : "—"}
                    </td>
                    <td>{event.cashPerTen}</td>
                    <td>{event.bonusPerTen}</td>
                    <td>{event.transferPerTen}</td>
                    <td>
                      {latest?.dividendEvents.some(
                        (e) =>
                          e.exDate === event.exDate &&
                          e.reportDate === event.reportDate,
                      )
                        ? "计入"
                        : "—"}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="watch-meta">
          {covered ? "近三年没有已实施的分红方案。" : "暂无可展示的分红事件。"}
        </p>
      )}
    </section>
  );
}
