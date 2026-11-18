import { useState } from "react";
import { Button, Input, Segmented, Select, Table } from "antd";
import type {
  PriceCollectionDashboard,
  PriceCollectionDay,
} from "../../api/activity";

type Row = PriceCollectionDashboard["rows"][number];
const statusOrder = ["success", "failed", "pending", "lagging"] as const;
type Status = (typeof statusOrder)[number];
const statuses: Record<Status | "inactive", string> = {
  success: "今日成功",
  failed: "获取失败",
  pending: "等待更新",
  lagging: "价格滞后",
  inactive: "停止跟踪",
};
const issues: Record<string, string> = {
  fetch_in_progress: "正在获取",
  fetch_interrupted: "采集中断，等待自动重试",
  source_failed: "来源获取失败，保留已有数据",
  history_pending: "等待首次回补",
  backfill_pending: "旧五年缓存，等待十年回补",
  latest_close_lagging: "落后于交易日参考，可能停牌或来源延迟",
  close_check_pending: "收盘截止已变化，等待本轮检查",
  today_pending: "今天尚未完成获取",
  missing_close_values: "部分日期缺少原价或前复权值",
  short_history: "历史不足十年，请核对上市时间与来源覆盖",
  trading_calendar_pending: "交易日参考尚未就绪，暂不判断滞后",
  tracking_inactive: "近 48 小时无持仓或观察登记，已停止刷新，保留已有价格",
};
const markets: Record<Row["market"], string> = {
  cn: "A股",
  hk: "港股",
  us: "美股",
  other: "其他市场",
};
const groupOrder = [
  "csi300",
  "cn-stock",
  "cn-fund",
  "hk-stock",
  "hk-fund",
  "us-stock",
  "us-fund",
  "other-stock",
  "other-fund",
  "reference",
];
const groups: Record<string, string> = {
  csi300: "沪深300成分",
  "cn-stock": "其他A股",
  "cn-fund": "A股基金/ETF",
  "hk-stock": "港股",
  "hk-fund": "港股基金/ETF",
  "us-stock": "美股",
  "us-fund": "美股ETF",
  "other-stock": "其他市场",
  "other-fund": "其他市场基金",
  reference: "交易日参考指数",
};
const isFund = (type: string) => ["FUND", "ETF", "MUTUALFUND"].includes(type);
const groupOf = (row: Row) =>
  row.scope === "other"
    ? `${row.market}-${isFund(row.instrumentType) ? "fund" : "stock"}`
    : row.scope;
const stamp = (value: number | null) =>
  value === null
    ? "尚无记录"
    : new Date(value * 1000).toLocaleString("zh-CN", {
        timeZone: "Asia/Shanghai",
        hour12: false,
      });
const counts = (day: PriceCollectionDay): Record<Status, number> => ({
  success: day.successful,
  failed: day.failed,
  pending: day.pending,
  lagging: day.lagging,
});

function StatusBar({ day, label }: { day: PriceCollectionDay; label: string }) {
  const values = counts(day);
  return (
    <div
      className="price-status-bar"
      role="img"
      aria-label={`${label}：${statusOrder
        .map((status) => `${statuses[status]} ${values[status]}`)
        .join("，")}`}
    >
      {statusOrder.map(
        (status) =>
          values[status] > 0 && (
            <span
              key={status}
              className={`price-status-${status}`}
              style={{ flexGrow: values[status] }}
            />
          ),
      )}
    </div>
  );
}

function StatusLabel({ status }: { status: string }) {
  return (
    <span className={`price-status-label price-status-${status}`}>
      {statuses[status as Status] ?? status}
    </span>
  );
}

function UniverseAlerts({ data }: { data: PriceCollectionDashboard }) {
  return (
    <>
      {!data.universe && (
        <p className="price-collection-alert">
          尚未取得有效的300只成分表，等待后台获取
        </p>
      )}
      {data.universeError && (
        <p className="price-collection-alert" role="status">
          成分表获取异常：{data.universeError}。保留上次范围；最近尝试{" "}
          {stamp(data.universeAttemptedAt)}。
        </p>
      )}
    </>
  );
}

interface GroupCount {
  key: string;
  tracked: number;
  success: number;
  failed: number;
  pending: number;
  lagging: number;
  inactive: number;
}
function groupCounts(rows: Row[]): GroupCount[] {
  const result = new Map<string, GroupCount>();
  for (const row of rows) {
    const key = groupOf(row);
    const entry = result.get(key) ?? {
      key,
      tracked: 0,
      success: 0,
      failed: 0,
      pending: 0,
      lagging: 0,
      inactive: 0,
    };
    if (row.tracked) {
      entry.tracked += 1;
      const status = row.status as Status;
      if (statusOrder.includes(status)) entry[status] += 1;
    } else entry.inactive += 1;
    result.set(key, entry);
  }
  return [...result.values()].sort(
    (a, b) => groupOrder.indexOf(a.key) - groupOrder.indexOf(b.key),
  );
}

function ScopeTable({ rows }: { rows: Row[] }) {
  return (
    <Table
      size="small"
      rowKey="key"
      aria-label="价格维护范围"
      dataSource={groupCounts(rows)}
      pagination={false}
      scroll={{ x: 640 }}
      locale={{ emptyText: "尚无维护中的证券" }}
      columns={[
        {
          title: "维护范围",
          dataIndex: "key",
          render: (key: string) => groups[key] ?? key,
        },
        { title: "跟踪中", dataIndex: "tracked", align: "right" },
        { title: "成功", dataIndex: "success", align: "right" },
        { title: "失败", dataIndex: "failed", align: "right" },
        { title: "待更新", dataIndex: "pending", align: "right" },
        { title: "滞后", dataIndex: "lagging", align: "right" },
        { title: "停止跟踪", dataIndex: "inactive", align: "right" },
      ]}
    />
  );
}

export function PriceSummary({
  data,
  onOpenDetail,
}: {
  data?: PriceCollectionDashboard;
  onOpenDetail: () => void;
}) {
  const summary = data && counts(data.summary);
  return (
    <section className="panel usage-panel" aria-label="公共价格采集概况">
      <div className="usage-table-heading">
        <h2>公共价格 · 十年日线采集</h2>
        <Button onClick={onOpenDetail}>查看明细</Button>
      </div>
      {data && <UniverseAlerts data={data} />}
      <div className="price-status-overview">
        {data && <StatusBar day={data.summary} label="今日价格状态分布" />}
        <div className="price-status-legend" aria-label="价格状态图例">
          {statusOrder.map((key) => (
            <div key={key}>
              <StatusLabel status={key} />
              <strong>{summary?.[key] ?? "—"}</strong>
            </div>
          ))}
        </div>
      </div>
      <ScopeTable rows={data?.rows ?? []} />
    </section>
  );
}

export function PriceCollection({
  data,
  loading,
}: {
  data?: PriceCollectionDashboard;
  loading: boolean;
}) {
  const [filter, setFilter] = useState("all");
  const [status, setStatus] = useState<Status | null>(null);
  const [group, setGroup] = useState("all");
  const [search, setSearch] = useState("");
  const summary = data && counts(data.summary);
  const groupOptions = groupCounts(data?.rows ?? []).map(({ key }) => ({
    label: groups[key] ?? key,
    value: key,
  }));
  const rows = data?.rows.filter((row) => {
    const needle = search.trim().toLowerCase();
    return (
      (filter === "all" ||
        row.status !== "success" ||
        row.issueCodes.includes("missing_close_values")) &&
      (!status || row.status === status) &&
      (group === "all" || groupOf(row) === group) &&
      `${row.symbol} ${row.name}`.toLowerCase().includes(needle)
    );
  });
  return (
    <section className="panel usage-panel" aria-label="公共价格采集">
      <h2>公共价格 · 十年日线采集</h2>
      {data && <UniverseAlerts data={data} />}
      <div className="price-status-overview">
        {data && <StatusBar day={data.summary} label="今日价格状态分布" />}
        <div
          className="price-status-legend"
          role="group"
          aria-label="价格状态图例"
        >
          {statusOrder.map((key) => (
            <button
              key={key}
              type="button"
              aria-pressed={status === key}
              onClick={() => setStatus(status === key ? null : key)}
            >
              <StatusLabel status={key} />
              <strong>{summary?.[key] ?? "—"}</strong>
            </button>
          ))}
        </div>
      </div>
      <h3>维护范围</h3>
      <ScopeTable rows={data?.rows ?? []} />
      <div className="usage-table-heading">
        <Segmented
          aria-label="价格状态筛选"
          value={filter}
          options={[
            { label: "全部证券", value: "all" },
            { label: "异常与待获取", value: "issues" },
          ]}
          onChange={setFilter}
        />
        <Select
          aria-label="维护范围筛选"
          value={group}
          onChange={setGroup}
          style={{ minWidth: 160 }}
          options={[{ label: "全部范围", value: "all" }, ...groupOptions]}
        />
        <Input.Search
          aria-label="搜索价格证券"
          placeholder="搜索证券名称或代码"
          allowClear
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
      </div>
      <Table
        size="small"
        loading={loading}
        rowKey="symbol"
        dataSource={rows ?? []}
        pagination={{ pageSize: 10, showSizeChanger: false }}
        scroll={{ x: 1320 }}
        locale={{ emptyText: "暂无符合条件的采集记录" }}
        columns={[
          {
            title: "证券",
            key: "security",
            fixed: "left",
            render: (_, row: Row) => (
              <>
                {row.name || row.symbol}
                <br />
                <small>
                  {[
                    row.name ? row.symbol : null,
                    markets[row.market],
                    row.instrumentType,
                    row.currency,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </small>
              </>
            ),
          },
          {
            title: "维护范围",
            key: "group",
            render: (_, row: Row) => groups[groupOf(row)] ?? groupOf(row),
          },
          {
            title: "当前状态",
            dataIndex: "status",
            render: (value: string) => <StatusLabel status={value} />,
          },
          {
            title: "实际价格区间",
            key: "coverage",
            width: 210,
            render: (_, row: Row) =>
              row.firstOn ? (
                <span
                  className="price-coverage"
                  title={`${row.bars} 条日线 · ${row.provider}`}
                >
                  {row.firstOn} 至 {row.latestOn}
                </span>
              ) : (
                "尚无价格"
              ),
          },
          {
            title: "最近尝试（北京时间）",
            dataIndex: "attemptedAt",
            render: stamp,
          },
          {
            title: "最近成功（北京时间）",
            dataIndex: "succeededAt",
            render: stamp,
          },
          {
            title: "异常与覆盖说明",
            key: "issues",
            width: 330,
            render: (_, row: Row) => (
              <>
                {row.error && <p>{row.error}</p>}
                {row.issueCodes.map((code) => (
                  <div key={code}>{issues[code] ?? code}</div>
                ))}
                {!row.error && row.issueCodes.length === 0 ? "无异常" : null}
              </>
            ),
          },
        ]}
      />
      <h3>每日采集结果</h3>
      <Table
        size="small"
        loading={loading}
        rowKey="date"
        dataSource={data?.daily ?? []}
        pagination={{ pageSize: 7, showSizeChanger: false }}
        scroll={{ x: 820 }}
        locale={{ emptyText: "启用后开始记录，不追溯补造过去的成功状态" }}
        columns={[
          { title: "日期（北京时间）", dataIndex: "date" },
          {
            title: "状态分布",
            key: "distribution",
            width: 220,
            render: (_, day: PriceCollectionDay) => (
              <StatusBar day={day} label={`${day.date} 价格状态分布`} />
            ),
          },
          { title: "跟踪证券", dataIndex: "expected", align: "right" },
          { title: "成功", dataIndex: "successful", align: "right" },
          { title: "失败", dataIndex: "failed", align: "right" },
          { title: "待获取", dataIndex: "pending", align: "right" },
          { title: "滞后", dataIndex: "lagging", align: "right" },
        ]}
      />
    </section>
  );
}
