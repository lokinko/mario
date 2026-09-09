import type { View } from "../../app/navigation";
import type { ReviewReminderSummary, Snapshot } from "../../types";

export interface NextAction {
  title: string;
  detail: string;
  destination?: View;
  label?: string;
}

export function nextAction(
  snapshot: Pick<Snapshot, "goals" | "valuationStatus" | "findings">,
  reminders: ReviewReminderSummary | null,
): NextAction {
  if (!snapshot.goals.length)
    return {
      title: "先确定资金的用途与期限",
      detail: "建立一个目标即可开始，不需要先配置模型或注册账户。",
      destination: "foundation",
      label: "设定第一个目标",
    };
  if (!snapshot.valuationStatus.comparable)
    return {
      title: "先补齐估值资料",
      detail: "当前口径不能可靠比较，补齐资料后再判断组合变化。",
      destination: "foundation",
      label: "完善持仓资料",
    };
  const risk = snapshot.findings.find((finding) => finding.level === "high");
  if (risk)
    return {
      title: risk.title,
      detail: risk.action,
      destination: "foundation",
      label: "检查财务与风险约束",
    };
  if (!reminders)
    return {
      title: "复盘状态尚未确认",
      detail: "尚未取得到期检查结果，不据此判断今天是否有待办。",
      destination: "review",
      label: "查看复盘状态",
    };
  if (reminders.dueDecisionCount > 0)
    return {
      title: `${reminders.dueDecisionCount} 条判断到了复盘时间`,
      detail: "先对照当时的假设与证伪条件，再决定是否调整。",
      destination: "review",
      label: "完成到期复盘",
    };
  if (reminders.periodicReviewDue)
    return {
      title: "做一次方法复盘",
      detail: "检查目标、纪律和需要修订的规则，不必生成新的交易。",
      destination: "review",
      label: "开始周期复盘",
    };
  return {
    title: "当前没有上述待办，不必为了使用工具而行动",
    detail:
      "这不是买卖建议，也不代表不存在其他风险。有新证据或约束变化时，再记录一次判断。",
  };
}
