import { useCallback, useEffect, useState } from "react";
import { Button, Input, Segmented, Table } from "antd";
import { Activity, ArrowLeft, RefreshCw } from "lucide-react";
import {
  getUsageDashboard,
  type UsageDashboard as DashboardData,
} from "../../api/activity";
import { useRequestGuard } from "../../lib/useRequestGuard";
import { HttpError } from "../../lib/transport";
import { Callout } from "../../components/Callout";
import "./dashboard.css";

const labels: Record<string, string> = {
  watch: "买点观察",
  calendar: "分红日历",
  facts: "我的情况",
  evidence: "参考资料",
  review: "复盘记录",
  advisor: "问答",
  methodology: "投资方法论",
  settings: "设置",
};
const stamp = (value: number | null) =>
  value === null
    ? "范围内无活动"
    : new Date(value * 1000).toLocaleString("zh-CN", {
        timeZone: "Asia/Shanghai",
        hour12: false,
      });

export function UsageDashboard() {
  const [days, setDays] = useState(30);
  const [data, setData] = useState<DashboardData>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [forbidden, setForbidden] = useState(false);
  const [search, setSearch] = useState("");
  const guard = useRequestGuard();
  const load = useCallback(async () => {
    const current = guard.begin();
    setLoading(true);
    setError("");
    try {
      const result = await getUsageDashboard(days);
      if (current()) {
        setData(result);
        setForbidden(false);
      }
    } catch (cause) {
      if (current()) {
        setData(undefined);
        setForbidden(cause instanceof HttpError && cause.status === 403);
        setError(
          cause instanceof Error ? cause.message : "统计加载失败，请重试",
        );
      }
    } finally {
      if (current()) setLoading(false);
    }
  }, [days, guard]);
  useEffect(() => {
    void load();
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void load();
    }, 60000);
    return () => {
      clearInterval(timer);
      guard.invalidate();
    };
  }, [load, guard]);
  const display = data?.days === days ? data : undefined;
  const users =
    display?.users.filter((row) =>
      row.email.toLowerCase().includes(search.trim().toLowerCase()),
    ) ?? [];
  return (
    <main className="usage-dashboard">
      <header className="usage-heading">
        <div>
          <a href={import.meta.env.BASE_URL}>
            <ArrowLeft size={14} />
            返回 Mario
          </a>
          <h1>
            <Activity aria-hidden="true" />
            使用活跃度
          </h1>
          <p>全站使用概况 · 仅超级账户可见</p>
        </div>
        <Button
          icon={<RefreshCw size={15} />}
          loading={loading}
          onClick={() => void load()}
        >
          刷新数据
        </Button>
      </header>
      {error && (
        <Callout
          tone="danger"
          title={forbidden ? "无权查看全站统计" : "无法加载活跃度"}
        >
          {error}
          {forbidden && (
            <p>请返回 Mario，在设置中切换到超级账户后重新打开此页。</p>
          )}
        </Callout>
      )}
      {!forbidden && (
        <>
          <section className="usage-metrics" aria-label="活跃度概览">
            {[
              ["注册账号", display?.totalUsers, "当前账号总数"],
              ["日活跃用户", display?.dailyActive, "今天"],
              ["周活跃用户", display?.weeklyActive, "含今天最近 7 天"],
              ["月活跃用户", display?.monthlyActive, "含今天最近 30 天"],
            ].map(([label, value, hint]) => (
              <article key={label}>
                <span>{label}</span>
                <strong>{value ?? "—"}</strong>
                <small>{hint}</small>
              </article>
            ))}
          </section>
          <div className="usage-toolbar">
            <Segmented
              aria-label="统计范围"
              value={days}
              options={[
                { label: "近 7 天", value: 7 },
                { label: "近 30 天", value: 30 },
                { label: "近 90 天", value: 90 },
              ]}
              onChange={(value) => setDays(Number(value))}
            />
            <span>
              {display
                ? `更新于 ${stamp(display.generatedAt)}`
                : "等待统计数据"}
            </span>
          </div>
          <section className="panel usage-panel">
            <h2>每日活跃</h2>
            <Table
              size="small"
              loading={loading}
              rowKey="date"
              dataSource={display?.daily ?? []}
              pagination={{ pageSize: 10, showSizeChanger: false }}
              scroll={{ x: 460 }}
              columns={[
                { title: "日期（北京时间）", dataIndex: "date" },
                { title: "活跃用户", dataIndex: "activeUsers", align: "right" },
                {
                  title: "活跃分钟",
                  dataIndex: "activeMinutes",
                  align: "right",
                },
              ]}
            />
          </section>
          <section className="panel usage-panel">
            <div className="usage-table-heading">
              <h2>用户活跃明细</h2>
              <Input.Search
                aria-label="搜索用户邮箱"
                placeholder="搜索用户邮箱"
                allowClear
                value={search}
                onChange={(event) => setSearch(event.target.value)}
              />
            </div>
            <Table
              size="small"
              loading={loading}
              rowKey="userId"
              dataSource={users}
              pagination={{ pageSize: 10, showSizeChanger: false }}
              scroll={{ x: 680 }}
              locale={{ emptyText: "暂无符合条件的账号" }}
              columns={[
                { title: "账号邮箱", dataIndex: "email" },
                {
                  title: "最近活动（范围内）",
                  dataIndex: "lastSeen",
                  render: stamp,
                },
                {
                  title: "活跃天数",
                  dataIndex: "activeDays",
                  align: "right",
                  sorter: (a, b) => a.activeDays - b.activeDays,
                },
                {
                  title: "活跃分钟",
                  dataIndex: "activeMinutes",
                  align: "right",
                  sorter: (a, b) => a.activeMinutes - b.activeMinutes,
                },
              ]}
            />
          </section>
          <section className="panel usage-panel">
            <h2>功能使用</h2>
            <Table
              size="small"
              loading={loading}
              rowKey="page"
              dataSource={display?.pages ?? []}
              pagination={false}
              scroll={{ x: 460 }}
              locale={{ emptyText: "尚无页面活动记录" }}
              columns={[
                {
                  title: "功能页面",
                  dataIndex: "page",
                  render: (page: string) => labels[page] ?? page,
                },
                { title: "活跃用户", dataIndex: "activeUsers", align: "right" },
                {
                  title: "页面活跃分钟",
                  dataIndex: "activeMinutes",
                  align: "right",
                },
              ]}
            />
          </section>
          <footer className="usage-definition">
            <h2>统计口径</h2>
            <p>
              活跃用户指登录后在可见页面中有活动采样的去重账号。每 30
              秒检查一次，连续 5
              分钟无点击、键盘、滚动或触摸后停止采样；后台同步不计入。
            </p>
            <p>
              活跃分钟是有采样的账号分钟数，并非精确在线时长或访问次数。同一账号多标签页去重；跨功能页的分钟可分别计入各页，因此功能行不能直接相加。看板自身不计入。
            </p>
            <p>
              按北京时间自然日统计，保留最近 90 天。
              {display && `采集启用于 ${stamp(display.trackingSince)}；`}
              启用前未采集，空白日期不代表过去无人使用。仅保存功能页标识和活动分钟，不采集财务与问答正文。页面每分钟自动刷新。
            </p>
          </footer>
        </>
      )}
    </main>
  );
}
