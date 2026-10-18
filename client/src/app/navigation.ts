import { MessageCircle, Database, History, WalletCards } from "lucide-react";

export type View =
  "facts" | "evidence" | "review" | "advisor" | "cloud" | "settings";

export const views: View[] = [
  "facts",
  "evidence",
  "review",
  "advisor",
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
    caption: "总览",
    items: [
      { id: "facts" as const, label: "我的情况", icon: WalletCards },
      { id: "advisor" as const, label: "问答", icon: MessageCircle },
    ],
  },
  {
    caption: "研究",
    items: [
      { id: "review" as const, label: "复盘记录", icon: History },
      { id: "evidence" as const, label: "参考资料", icon: Database },
    ],
  },
];

export const viewLabels: Record<View, string> = {
  advisor: "问答",
  facts: "我的情况",
  review: "复盘记录",
  evidence: "参考资料",
  cloud: "账号与数据",
  settings: "设置",
};
