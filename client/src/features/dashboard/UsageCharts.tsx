import type { UsageDashboard } from "../../api/activity";

const pageLabels: Record<string, string> = {
  watch: "买点观察",
  calendar: "分红日历",
  facts: "我的情况",
  evidence: "参考资料",
  review: "复盘记录",
  advisor: "问答",
  methodology: "投资方法论",
  settings: "设置",
};

const width = 640;
const height = 160;
const top = 8;
const bottom = 1;

export function UserGrowth({ data }: { data?: UsageDashboard }) {
  const days = [...(data?.registrations ?? [])].reverse();
  const added = days.reduce((sum, day) => sum + day.newUsers, 0);
  const total = days.at(-1)?.totalUsers ?? 0;
  const maxNew = Math.max(1, ...days.map((day) => day.newUsers));
  const maxTotal = Math.max(1, ...days.map((day) => day.totalUsers));
  const step = width / Math.max(1, days.length);
  const plot = height - top - bottom;
  const line = days
    .map(
      (day, index) =>
        `${(index + 0.5) * step},${top + plot * (1 - day.totalUsers / maxTotal)}`,
    )
    .join(" ");
  return (
    <section className="panel usage-panel" aria-label="用户增长">
      <h2>用户增长</h2>
      <div className="growth-legend">
        <span className="growth-key growth-key-new">每日新注册</span>
        <span className="growth-key growth-key-total">
          累计账号 <strong>{data ? total : "—"}</strong>
        </span>
        <span>
          范围内新增 <strong>{data ? added : "—"}</strong>
        </span>
        <span>
          上线前已有 <strong>{data?.registeredBeforeTracking ?? "—"}</strong>
        </span>
      </div>
      {days.length > 0 && (
        <svg
          className="growth-chart"
          viewBox={`0 0 ${width} ${height}`}
          preserveAspectRatio="none"
          role="img"
          aria-label={`${days[0].date} 至 ${days.at(-1)?.date}，新注册 ${added} 个，累计账号 ${total} 个，其中上线前已有 ${data?.registeredBeforeTracking ?? 0} 个`}
        >
          <line
            className="growth-axis"
            x1={0}
            x2={width}
            y1={height - bottom}
            y2={height - bottom}
          />
          {days.map((day, index) => {
            const barHeight = (plot * day.newUsers) / maxNew;
            return (
              <rect
                key={day.date}
                className="growth-bar"
                x={index * step + step * 0.2}
                y={height - bottom - barHeight}
                width={step * 0.6}
                height={barHeight}
              >
                <title>{`${day.date}：新注册 ${day.newUsers}，累计 ${day.totalUsers}`}</title>
              </rect>
            );
          })}
          <polyline className="growth-line" points={line} />
        </svg>
      )}
      {days.length > 0 && (
        <div className="growth-dates">
          <span>{days[0].date}</span>
          <span>{days.at(-1)?.date}</span>
        </div>
      )}
    </section>
  );
}

export function FeatureUsage({ data }: { data?: UsageDashboard }) {
  const pages = [...(data?.pages ?? [])].sort(
    (a, b) => b.activeMinutes - a.activeMinutes,
  );
  const max = Math.max(1, ...pages.map((page) => page.activeMinutes));
  return (
    <section className="panel usage-panel" aria-label="功能使用">
      <h2>功能使用</h2>
      {data && pages.length === 0 && (
        <p className="usage-empty">尚无页面活动记录</p>
      )}
      <ul className="feature-usage">
        {pages.map((page) => (
          <li key={page.page}>
            <span>{pageLabels[page.page] ?? page.page}</span>
            <span className="feature-usage-track">
              <span style={{ width: `${(page.activeMinutes / max) * 100}%` }} />
            </span>
            <span>
              {page.activeMinutes} 分钟 · {page.activeUsers} 人
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}
