import {
  MessageCircle,
  Database,
  History,
  WalletCards,
  BookOpen,
} from "lucide-react";

export type View =
  | "facts"
  | "evidence"
  | "review"
  | "advisor"
  | "methodology"
  | "cloud"
  | "settings";

export const views: View[] = [
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
  return views.includes(candidate) ? candidate : "facts";
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
    items: [{ id: "advisor", label: "问答", icon: MessageCircle }],
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

export const viewLabels: Record<View, string> = {
  methodology: "投资方法论",
  advisor: "问答",
  facts: "我的情况",
  review: "复盘记录",
  evidence: "参考资料",
  cloud: "账号与数据",
  settings: "设置",
};
