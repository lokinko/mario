import {
  BookMarked,
  MessageCircle,
  CircleDollarSign,
  Database,
  FilePenLine,
  History,
  LayoutDashboard,
  WalletCards,
} from "lucide-react";

export type View =
  | "dashboard"
  | "foundation"
  | "ledger"
  | "evidence"
  | "decision"
  | "review"
  | "memory"
  | "advisor"
  | "cloud"
  | "settings";

export type FoundationSection = "holdings" | "profile" | "goals";

export const views: View[] = [
  "dashboard",
  "foundation",
  "ledger",
  "evidence",
  "decision",
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
];

export const supportingNav = [
  { id: "decision" as const, label: "决策确认", icon: FilePenLine },
  { id: "review" as const, label: "复盘记录", icon: History },
  { id: "foundation" as const, label: "持仓与目标", icon: WalletCards },
  { id: "dashboard" as const, label: "资产概览", icon: LayoutDashboard },
  { id: "ledger" as const, label: "组合流水", icon: CircleDollarSign },
  { id: "evidence" as const, label: "参考资料", icon: Database },
  { id: "memory" as const, label: "记忆与规则", icon: BookMarked },
];
