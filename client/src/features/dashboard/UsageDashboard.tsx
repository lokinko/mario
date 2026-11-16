import { PriceCollection, PriceSummary } from "./PriceCollection";
import { FeatureUsage, UserGrowth } from "./UsageCharts";
import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Input, Segmented, Table } from "antd";
import {
  Activity,
  ArrowLeft,
  LayoutDashboard,
  ListChecks,
  RefreshCw,
  Users,
} from "lucide-react";
import {
  getUsageDashboard,
  type UsageDashboard as DashboardData,
} from "../../api/activity";
import { useRequestGuard } from "../../lib/useRequestGuard";
import { HttpError } from "../../lib/transport";
import { currentSession } from "../../lib/webSession";
import { Callout } from "../../components/Callout";
import "./dashboard.css";

type View = "overview" | "users" | "prices";
const viewFromHash = (): View =>
  window.location.hash === "#users"
    ? "users"
    : window.location.hash === "#prices"
      ? "prices"
      : "overview";
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
  const [view, setView] = useState(viewFromHash);
  useEffect(() => {
    const sync = () => setView(viewFromHash());
    window.addEventListener("hashchange", sync);
    return () => window.removeEventListener("hashchange", sync);
  }, []);
  const open = (next: View) => {
    const { pathname, search: query } = window.location;
    window.history.pushState(
      null,
      "",
      next === "overview" ? pathname + query : `#${next}`,
    );
    setView(next);
  };
  const guard = useRequestGuard();
  const activeSession = useRef(currentSession());
  const load = useCallback(async () => {
    const current = guard.begin();
    const session = currentSession();
    if (activeSession.current !== session) setData(undefined);
    activeSession.current = session;
    setLoading(true);
    setError("");
    try {
      const result = await getUsageDashboard(days);
      if (current() && currentSession() === session) {
        setData(result);
        setForbidden(false);
      }
    } catch (cause) {
      if (current()) {
        const denied =
          currentSession() !== session ||
          (cause instanceof HttpError &&
            (cause.status === 401 || cause.status === 403));
        if (denied) setData(undefined);
        setForbidden(denied);
        setError(
          cause instanceof Error ? cause.message : "统计加载失败，请重试",
        );
      }
    } finally {
      if (current()) setLoading(false);
    }
  }, [days, guard]);
  useEffect(() => {
    const expire = () => {
      guard.invalidate();
      setData(undefined);
      setLoading(false);
      setForbidden(true);
      setError("登录状态已失效，请返回 Mario 重新登录。");
    };
    const syncSession = () => {
      if (currentSession() !== activeSession.current) {
        setData(undefined);
        void load();
      }
    };
    window.addEventListener("storage", syncSession);
    window.addEventListener("focus", syncSession);
    window.addEventListener("mario:session-expired", expire);
    return () => {
      window.removeEventListener("storage", syncSession);
      window.removeEventListener("focus", syncSession);
      window.removeEventListener("mario:session-expired", expire);
    };
  }, [load, guard]);
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
            运行看板
          </h1>
        </div>
        <div className="usage-heading-actions">
          {forbidden ? null : view === "overview" ? (
            <>
              <Button icon={<Users size={15} />} onClick={() => open("users")}>
                用户活跃明细
              </Button>
              <Button
                icon={<ListChecks size={15} />}
                onClick={() => open("prices")}
              >
                价格采集明细
              </Button>
            </>
          ) : (
            <Button
              icon={<LayoutDashboard size={15} />}
              onClick={() => open("overview")}
            >
              返回总览
            </Button>
          )}
          <Button
            icon={<RefreshCw size={15} />}
            loading={loading}
            onClick={() => void load()}
          >
            刷新数据
          </Button>
        </div>
      </header>
      {error && (
        <Callout
          tone="danger"
          title={forbidden ? "无权查看全站统计" : "看板刷新失败"}
        >
          {error}
          {forbidden && (
            <p>请返回 Mario，在设置中切换到超级账户后重新打开此页。</p>
          )}
        </Callout>
      )}
      {!forbidden && (
        <>
          {view === "overview" && (
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
              <UserGrowth data={display} />
              <FeatureUsage data={display} />
              <PriceSummary
                data={display?.marketPrices}
                onOpenDetail={() => open("prices")}
              />
            </>
          )}
          {view === "users" && (
            <>
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
                    {
                      title: "活跃用户",
                      dataIndex: "activeUsers",
                      align: "right",
                    },
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
            </>
          )}
          {view === "prices" && (
            <PriceCollection data={display?.marketPrices} loading={loading} />
          )}
        </>
      )}
    </main>
  );
}
