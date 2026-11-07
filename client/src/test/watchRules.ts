import type { WatchRules } from "../api/watch";
export const defaultRules: WatchRules = {
  minYieldBps: 500,
  daily: { reference: "lower", mode: "near", offsetBps: 200 },
  weekly: { reference: "middle", mode: "at_or_below", offsetBps: 0 },
};
