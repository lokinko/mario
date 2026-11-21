import {
  MessageCircle,
  Database,
  WalletCards,
  BookOpen,
  ChartNoAxesCombined,
  Newspaper,
} from "lucide-react";
import { dividendQuestionSymbol } from "../features/watch/dividend-preview/route";

export type View =
  | "watch"
  | "dailyReview"
  | "facts"
  | "evidence"
  | "advisor"
  | "methodology"
  | "settings";

export const views: View[] = [
  "watch",
  "dailyReview",
  "facts",
  "evidence",
  "advisor",
  "methodology",
  "settings",
];

export function initialView(): View {
  const candidate = window.location.hash.replace("#", "") as View;
  if (views.includes(candidate)) return candidate;
  return dividendQuestionSymbol(window.location.search) ? "advisor" : "watch";
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
      { id: "dailyReview", label: "每日复盘", icon: Newspaper },
      { id: "watch", label: "买点观察", icon: ChartNoAxesCombined },
      { id: "advisor", label: "问答", icon: MessageCircle },
    ],
  },
  {
    caption: "研究",
    items: [
      { id: "evidence", label: "研究资料", icon: Database },
      { id: "methodology", label: "投资方法论", icon: BookOpen },
    ],
  },
];

export const mobileTabs: (typeof navGroups)[number]["items"] = [
  { id: "dailyReview", label: "复盘", icon: Newspaper },
  { id: "watch", label: "观察", icon: ChartNoAxesCombined },
  { id: "facts", label: "资产", icon: WalletCards },
  { id: "advisor", label: "问答", icon: MessageCircle },
];

export const viewLabels: Record<View, string> = {
  dailyReview: "每日复盘",
  watch: "买点观察",
  methodology: "投资方法论",
  advisor: "问答",
  facts: "我的情况",
  evidence: "研究资料",
  settings: "设置",
};
