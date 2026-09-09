import type { Snapshot } from "../../types";

export function goalStatus(
  status: Snapshot["plan"]["goalProjections"][number]["status"],
) {
  return {
    "on-track": "路径较稳",
    watch: "需要关注",
    "off-track": "存在缺口",
    reached: "已经达成",
    expired: "目标到期",
  }[status];
}

export function riskStatus(status: Snapshot["plan"]["riskStatus"]) {
  return {
    within: "边界内",
    near: "接近上限",
    over: "超出边界",
    insufficient: "等待持仓",
  }[status];
}

export function allocationGradient(allocation: { pct: number }[]) {
  const colors = ["#cf5c3b", "#23443b", "#c99a45", "#80948f", "#774936"];
  let cursor = 0;
  const stops = allocation.map((item, index) => {
    const start = cursor;
    cursor += item.pct;
    return `${colors[index % colors.length]} ${start}% ${cursor}%`;
  });
  return `conic-gradient(${stops.join(",")})`;
}
