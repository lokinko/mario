import {
  MessageCircle,
  Database,
  History,
  WalletCards,
  BookOpen,
  ChartNoAxesCombined,
  ChartPie,
  PiggyBank,
} from "lucide-react";
import type { FactsSection } from "../features/portfolio/MyFacts";

export type View =
  | "watch"
  | "facts"
  | "evidence"
  | "review"
  | "advisor"
  | "methodology"
  | "cloud"
  | "settings";

export const views: View[] = [
  "watch",
  "facts",
  "evidence",
  "review",
  "advisor",
  "methodology",
  "cloud",
  "settings",
];

export function initialView(): View {
  const candidate = window.location.hash.replace("#", "") as View;
  return views.includes(candidate) ? candidate : "watch";
}

export const navGroups: {
  caption: string;
  items: { id: View; label: string; icon: typeof MessageCircle }[];
}[] = [
  {
    caption: "资产",
    items: [{ id: "facts", label: "我的情况", icon: WalletCards }],
  },
  {
    caption: "决策",
    items: [
      { id: "watch", label: "买点观察", icon: ChartNoAxesCombined },
      { id: "advisor", label: "问答", icon: MessageCircle },
    ],
  },
  {
    caption: "研究",
    items: [
      { id: "evidence", label: "参考资料", icon: Database },
      { id: "review", label: "复盘记录", icon: History },
      { id: "methodology", label: "投资方法论", icon: BookOpen },
    ],
  },
];

export const mobileTabs: {
  id: string;
  view: View;
  section?: FactsSection;
  label: string;
  icon: typeof MessageCircle;
}[] = [
  { id: "watch", view: "watch", label: "买点", icon: ChartNoAxesCombined },
  {
    id: "holdings",
    view: "facts",
    section: "holdings",
    label: "持仓",
    icon: ChartPie,
  },
  {
    id: "savings",
    view: "facts",
    section: "savings",
    label: "存款",
    icon: PiggyBank,
  },
  { id: "advisor", view: "advisor", label: "问答", icon: MessageCircle },
];

export const viewLabels: Record<View, string> = {
  watch: "买点观察",
  methodology: "投资方法论",
  advisor: "问答",
  facts: "我的情况",
  review: "复盘记录",
  evidence: "参考资料",
  cloud: "账号与数据",
  settings: "设置",
};
