import {
  MessageCircle,
  Database,
  History,
  WalletCards,
  LayoutDashboard,
} from "lucide-react";

export type View =
  | "facts"
  | "dashboard"
  | "foundation"
  | "ledger"
  | "evidence"
  | "review"
  | "memory"
  | "advisor"
  | "cloud"
  | "settings";

export type FoundationSection = "holdings" | "profile" | "goals";

export const views: View[] = [
  "facts",
  "dashboard",
  "foundation",
  "ledger",
  "evidence",
  "review",
  "memory",
  "advisor",
  "cloud",
  "settings",
];

export function initialView(): View {
  const candidate = window.location.hash.replace("#", "") as View;
  return views.includes(candidate) ? candidate : "advisor";
}

export const nav = [
  { id: "advisor" as const, label: "问答", icon: MessageCircle },
  { id: "facts" as const, label: "我的情况", icon: WalletCards },
  { id: "dashboard" as const, label: "分析看板", icon: LayoutDashboard },
  { id: "review" as const, label: "复盘记录", icon: History },
  { id: "evidence" as const, label: "参考资料", icon: Database },
];

export const viewLabels: Record<View, string> = {
  advisor: "问答",
  facts: "我的情况",
  review: "复盘记录",
  evidence: "参考资料",
  foundation: "持仓与目标",
  dashboard: "分析看板",
  ledger: "组合流水",
  memory: "记忆与规则",
  cloud: "账号与数据",
  settings: "设置",
};
