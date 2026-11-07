import type { WatchBoard, WatchRow } from "../../api/watch";
import { rulesSummary } from "./rules";
import { statuses } from "./presentation";

export function WatchWeekly({
  board,
  onAsk,
  onDetails,
}: {
  board: WatchBoard;
  onAsk: () => void;
  onDetails: (row: WatchRow) => void;
}) {
  const days = Array.from({ length: 7 }, (_, i) => {
    const day = new Date(`${board.weekStart}T00:00:00Z`);
    day.setUTCDate(day.getUTCDate() + i);
    return day.toISOString().slice(0, 10);
  });
  return (
    <section className="panel watch-weekly" aria-label="本周观察足迹">
      <div className="watch-section-title">
        <div className="watch-week-legend">
          <span className="attention">关注</span>
          <span className="waiting">等待</span>
          <span className="incomplete">待补齐</span>
          <span>— 未保存</span>
        </div>
        <button
          className="secondary"
          disabled={!board.rows.length}
          onClick={onAsk}
        >
          结合持仓做周末复盘{board.rows.length > 5 ? "（先讨论前 5 家）" : ""}
        </button>
      </div>
      <p className="watch-meta">
        历史状态使用检查当时的规则，悬停查看该记录的规则。
      </p>
      {board.rows.length > 0 && (
        <div className="table-scroll">
          <table className="watch-week-table" aria-label="本周逐日观察">
            <thead>
              <tr>
                <th>公司</th>
                {days.map((day, i) => (
                  <th key={day}>
                    {["一", "二", "三", "四", "五", "六", "日"][i]}
                    <small>{day.slice(5)}</small>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {board.rows.map((row) => (
                <tr key={row.entry.symbol}>
                  <th>
                    <button
                      className="text-button"
                      onClick={() => onDetails(row)}
                    >
                      {row.entry.name}
                    </button>
                  </th>
                  {days.map((day) => {
                    const record = board.weekly.find(
                      (v) =>
                        v.symbol === row.entry.symbol && v.observedOn === day,
                    );
                    return (
                      <td key={day}>
                        <span
                          className={`watch-week-cell ${record?.status ?? ""}`}
                          title={`${row.entry.name} ${day} ${record ? `${statuses[record.status]}；${rulesSummary(record.rules)}` : "没有保存的检查结果"}`}
                        >
                          {record
                            ? {
                                attention: "关注",
                                waiting: "等待",
                                incomplete: "待补齐",
                              }[record.status]
                            : "—"}
                        </span>
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
