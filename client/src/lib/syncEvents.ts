export const DATA_SAVED = "mario:data-saved";
export const SERVER_SESSION_CHANGED = "mario:session-changed";
export const SERVER_DATA_UPDATED = "mario:server-data-updated";
let remoteUpdatePending = false;
let running: Promise<void> | undefined;
let finish: (() => void) | undefined;
let count = 0;
export function beginSync() {
  count++;
  if (running) return;
  running = new Promise<void>((resolve) => {
    finish = resolve;
  });
}
export function endSync() {
  count = Math.max(0, count - 1);
  if (count) return;
  finish?.();
  running = undefined;
  finish = undefined;
}
export function blockStaleWrites(value: boolean) {
  remoteUpdatePending = value;
}
export async function beforeDataWrite() {
  await running;
  if (remoteUpdatePending)
    throw new Error(
      "其他设备已更新资料。请先点击‘查看最新资料’，核对后再保存；你的输入仍保留在页面中。",
    );
}
export function isDataWrite(path: string, method = "GET") {
  return (
    method !== "GET" && !path.endsWith("/preview") && !path.endsWith("/test")
  );
}

let dataWrites = 0;
export function markDataWrite() {
  dataWrites++;
  return () => {
    dataWrites--;
  };
}
export function hasDataWrites() {
  return dataWrites > 0;
}
