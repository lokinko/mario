import { useState } from "react";
import { Input, Segmented, Table, Tag } from "antd";
import type { PriceCollectionDashboard } from "../../api/activity";

const statuses: Record<string, { label: string; color: string }> = {
  success: { label: "今日成功", color: "success" },
  failed: { label: "获取失败", color: "error" },
  pending: { label: "等待更新", color: "default" },
  lagging: { label: "价格滞后", color: "warning" },
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
};
const stamp = (value: number | null) =>
  value === null
    ? "尚无记录"
    : new Date(value * 1000).toLocaleString("zh-CN", {
        timeZone: "Asia/Shanghai",
        hour12: false,
      });

export function PriceCollection({
  data,
  loading,
}: {
  data?: PriceCollectionDashboard;
  loading: boolean;
}) {
  const [filter, setFilter] = useState("all");
  const [search, setSearch] = useState("");
  const rows = data?.rows.filter((row) => {
    const needle = search.trim().toLowerCase();
    return (
      (filter === "all" ||
        row.status !== "success" ||
        row.issueCodes.includes("missing_close_values")) &&
      `${row.symbol} ${row.name}`.toLowerCase().includes(needle)
    );
  });
  return (
    <section className="panel usage-panel" aria-label="沪深300价格采集">
      <h2>沪深300 · 十年价格采集</h2>
      <p className="price-collection-meta">
        当前成分股各自过去十年的日收盘价和前复权价；成分表每日检查，价格每五分钟检查，分批回补。
        上市不足十年的只保留实际历史，不补造价格。
      </p>
      <p className="price-collection-meta">
        {data?.universe ? (
          <>
            成分表日期 {data.universe.asOf} · {data.universe.members.length} 只
            ·{" "}
            <a href={data.universe.sourceUrl} target="_blank" rel="noreferrer">
              中证官方成分表
            </a>
            {" · "}最近获取 {stamp(data.universe.fetchedAt)}
          </>
        ) : (
          "尚未取得有效的300只成分表，等待后台获取"
        )}
        {data && `；价格收盘截止 ${data.targetThrough}`}
      </p>
      {data?.universeError && (
        <p role="status">
          成分表获取异常：{data.universeError}。保留上次范围；最近尝试{" "}
          {stamp(data.universeAttemptedAt)}。
        </p>
      )}
      <div className="usage-metrics" aria-label="价格获取概览">
        {[
          ["今日成功", data?.summary.successful, "完成获取；价格覆盖见明细"],
          ["获取失败", data?.summary.failed, "失败或上次采集中断"],
          ["等待更新", data?.summary.pending, "首次回补、十年扩展或今日待获取"],
          ["价格滞后", data?.summary.lagging, "落后于来源实际交易日参考"],
        ].map(([label, count, hint]) => (
          <article key={label}>
            <span>{label}</span>
            <strong>{count ?? "—"}</strong>
            <small>{hint}</small>
          </article>
        ))}
      </div>
      <div className="usage-table-heading">
        <Segmented
          aria-label="价格状态筛选"
          value={filter}
          options={[
            { label: "全部股票", value: "all" },
            { label: "异常与待获取", value: "issues" },
          ]}
          onChange={setFilter}
        />
        <Input.Search
          aria-label="搜索价格证券"
          placeholder="搜索股票名称或代码"
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
        scroll={{ x: 1200 }}
        locale={{ emptyText: "暂无符合条件的采集记录" }}
        columns={[
          {
            title: "股票",
            key: "security",
            fixed: "left",
            render: (_, row) => (
              <>
                {row.name}
                <br />
                <small>{row.symbol}</small>
              </>
            ),
          },
          {
            title: "当前状态",
            dataIndex: "status",
            render: (status: string) => (
              <Tag color={statuses[status]?.color}>
                {statuses[status]?.label ?? status}
              </Tag>
            ),
          },
          {
            title: "实际价格区间",
            key: "coverage",
            render: (_, row) =>
              row.firstOn ? (
                <>
                  {row.firstOn} 至 {row.latestOn}
                  <br />
                  {row.bars} 条日线 · {row.provider}
                </>
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
            render: (_, row) => (
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
        scroll={{ x: 650 }}
        locale={{ emptyText: "启用后开始记录，不追溯补造过去的成功状态" }}
        columns={[
          { title: "日期（北京时间）", dataIndex: "date" },
          { title: "监控股票", dataIndex: "expected", align: "right" },
          { title: "成功", dataIndex: "successful", align: "right" },
          { title: "失败", dataIndex: "failed", align: "right" },
          { title: "待获取", dataIndex: "pending", align: "right" },
          { title: "滞后", dataIndex: "lagging", align: "right" },
        ]}
      />
      <p className="price-collection-meta">
        每日结果为该日最后保存的采集状态，今天显示实时状态；保留最近90天，停机日期不补报成功。失败不会清空旧价格。刷新数据只重读看板，后台自动重试；页面每分钟更新。
      </p>
    </section>
  );
}
