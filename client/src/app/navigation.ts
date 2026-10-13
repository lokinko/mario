import {
  MessageCircle,
  Database,
  History,
  WalletCards,
  Target,
  ArrowLeftRight,
  BookMarked,
} from "lucide-react";

export type View =
  | "facts"
  | "goals"
  | "ledger"
  | "evidence"
  | "review"
  | "memory"
  | "advisor"
  | "cloud"
  | "settings";

export const views: View[] = [
  "facts",
  "goals",
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
  if ((candidate as string) === "foundation") return "goals";
  return views.includes(candidate) ? candidate : "advisor";
}

export const navGroups: {
  caption: string;
  items: { id: View; label: string; icon: typeof MessageCircle }[];
}[] = [
  {
    caption: "总览",
    items: [
      { id: "advisor" as const, label: "问答", icon: MessageCircle },
      { id: "facts" as const, label: "我的情况", icon: WalletCards },
    ],
  },
  {
    caption: "投资",
    items: [
      { id: "goals", label: "投资目标", icon: Target },
      { id: "ledger" as const, label: "组合流水", icon: ArrowLeftRight },
    ],
  },
  {
    caption: "研究",
    items: [
      { id: "review" as const, label: "复盘记录", icon: History },
      { id: "evidence" as const, label: "参考资料", icon: Database },
      { id: "memory", label: "长期记忆", icon: BookMarked },
    ],
  },
];

export const viewLabels: Record<View, string> = {
  advisor: "问答",
  facts: "我的情况",
  review: "复盘记录",
  evidence: "参考资料",
  goals: "投资目标",
  ledger: "组合流水",
  memory: "长期记忆",
  cloud: "账号与数据",
  settings: "设置",
};
