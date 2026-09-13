export const DATA_SAVED = "mario:data-saved";
export const CLOUD_CHANGED = "mario:cloud-changed";
export const CLOUD_DATA_UPDATED = "mario:cloud-data-updated";
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
    method !== "GET" &&
    /^\/(daily-assets|profile|holdings|goals|decisions|investment-rules|memories|system-reviews|portfolio-checkins|portfolio-events|research-evidence|analysis)(\/|$)/.test(
      path,
    ) &&
    path !== "/analysis/preview" &&
    !path.endsWith("/preview")
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
