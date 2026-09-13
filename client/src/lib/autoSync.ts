import { beginSync, endSync, hasDataWrites } from "./syncEvents";
import { HttpError } from "./transport";
import type { AutoSyncResult } from "../types";
export interface SyncState {
  phase: "waiting" | "syncing" | "synced" | "offline" | "paused" | "attention";
  message: string;
}
export interface AutoSyncOptions {
  sync: () => Promise<AutoSyncResult>;
  visible: () => boolean;
  online: () => boolean;
  onStatus: (state: SyncState) => void;
  onRemoteUpdate: () => Promise<void>;
}
/** One foreground timer, one request in flight. SQLite remains the durable outbox. */
export function startAutoSync(options: AutoSyncOptions) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false,
    busy = false,
    pending = false,
    suspended = false;
  let needsRefresh = false,
    serverBusy = false;
  let failures = 0,
    idle = 0,
    lastStart = 0,
    firstChange = 0;
  const status = (phase: SyncState["phase"], message: string) => {
    if (!disposed) options.onStatus({ phase, message });
  };
  const cancel = () => {
    clearTimeout(timer);
    timer = undefined;
  };
  const schedule = (delay: number) => {
    cancel();
    if (!disposed && !suspended && options.visible() && options.online())
      timer = setTimeout(() => void run(), delay);
  };
  const run = async () => {
    if (disposed || busy || suspended || !options.visible()) return;
    if (!options.online()) {
      status("offline", "已保存在本机，联网后同步");
      return;
    }
    if (hasDataWrites()) {
      status("waiting", "本地保存完成后自动同步");
      schedule(1500);
      return;
    }
    busy = true;
    serverBusy = false;
    pending = false;
    firstChange = 0;
    lastStart = Date.now();
    beginSync();
    status("syncing", "正在同步…");
    try {
      const result = await options.sync();
      needsRefresh ||= Boolean(result.sync?.localUpdated);
      if (needsRefresh) {
        await options.onRemoteUpdate();
        needsRefresh = false;
      }
      if (disposed) return;
      serverBusy = result.state === "busy";
      failures = 0;
      if (serverBusy) {
        status("waiting", "本地操作完成后自动同步");
      } else if (result.state !== "synced") {
        suspended = true;
        status(
          "paused",
          result.state === "disabled" ? "自动同步已关闭" : "登录后自动同步",
        );
      } else {
        idle = result.sync?.localUpdated ? 0 : Math.min(idle + 1, 3);
        status("synced", "已同步");
      }
    } catch (error) {
      if (disposed) return;
      if (
        error instanceof HttpError &&
        [400, 401, 409].includes(error.status)
      ) {
        suspended = true;
        status("attention", String(error.message));
      } else {
        failures++;
        status("offline", "暂时无法同步，数据已保存在本机，将自动重试");
      }
    } finally {
      endSync();
      busy = false;
      if (!disposed)
        schedule(
          serverBusy
            ? 5000
            : pending
              ? 1500
              : failures
                ? Math.min(10000 * 2 ** Math.min(failures - 1, 5), 300000)
                : [60000, 60000, 120000, 300000][idle],
        );
    }
  };
  const changed = () => {
    idle = 0;
    pending = true;
    if (!firstChange) firstChange = Date.now();
    if (!suspended) status("waiting", "已保存在本机，等待同步");
    if (!busy)
      schedule(Math.max(0, Math.min(1500, 10000 - (Date.now() - firstChange))));
  };
  const wake = () => {
    cancel();
    if (!options.online()) {
      status("offline", "已保存在本机，联网后同步");
      return;
    }
    if (!busy) schedule(Math.max(0, 10000 - (Date.now() - lastStart)));
  };
  const retry = () => {
    suspended = false;
    failures = 0;
    idle = 0;
    pending = true;
    if (!busy) schedule(0);
  };
  if (!options.online()) status("offline", "已保存在本机，联网后同步");
  schedule(1000);
  return {
    changed,
    wake,
    retry,
    dispose: () => {
      disposed = true;
      cancel();
    },
  };
}
