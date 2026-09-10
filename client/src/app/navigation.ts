import {
  BookMarked,
  BrainCircuit,
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
  return views.includes(candidate) ? candidate : "dashboard";
}

export const nav = [
  { id: "dashboard" as const, label: "现在", icon: LayoutDashboard },
  { id: "decision" as const, label: "决策", icon: FilePenLine },
  { id: "review" as const, label: "复盘", icon: History },
];

export const supportingNav = [
  { id: "foundation" as const, label: "财务底座", icon: WalletCards },
  { id: "ledger" as const, label: "组合流水", icon: CircleDollarSign },
  { id: "evidence" as const, label: "研究证据", icon: Database },
  { id: "memory" as const, label: "长期记忆", icon: BookMarked },
  { id: "advisor" as const, label: "AI 研究室", icon: BrainCircuit },
];
