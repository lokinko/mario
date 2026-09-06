import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import {
  acknowledgeReviewReminder,
  getReviewReminders,
  saveReminderSettings,
} from "./api";
import type { ReminderSettings, ReviewReminderSummary } from "./types";

export function isDesktopApp(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function notificationBody(summary: ReviewReminderSummary): string {
  const parts: string[] = [];
  if (summary.dueDecisionCount > 0) parts.push(`${summary.dueDecisionCount} 条投资决策待复盘`);
  if (summary.periodicReviewDue) parts.push("周期复盘已到期");
  return `${parts.join("，")}。打开知衡按原始判断校准方法。`;
}

export async function checkAndSendReviewReminder(): Promise<ReviewReminderSummary> {
  const summary = await getReviewReminders();
  if (!isDesktopApp() || !summary.shouldNotify || !(await isPermissionGranted())) return summary;

  sendNotification({ title: "知衡 · 复盘提醒", body: notificationBody(summary) });
  return acknowledgeReviewReminder(summary.fingerprint);
}

export async function enableReviewReminders(): Promise<ReminderSettings> {
  if (!isDesktopApp()) throw new Error("系统通知只在知衡桌面应用中可用");
  let granted = await isPermissionGranted();
  if (!granted) granted = (await requestPermission()) === "granted";
  if (!granted) throw new Error("系统没有授予通知权限；你仍可在复盘中心查看到期事项");
  return saveReminderSettings(true);
}

export async function disableReviewReminders(): Promise<ReminderSettings> {
  return saveReminderSettings(false);
}
