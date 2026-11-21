import { initialView } from "./navigation";

let buyWatchPromise: Promise<{
  default: typeof import("../features/watch/BuyWatch").BuyWatch;
}> | null = null;
let myFactsPromise: Promise<{
  default: typeof import("../features/portfolio/MyFacts").MyFacts;
}> | null = null;

export function loadBuyWatch() {
  buyWatchPromise ??= import("../features/watch/BuyWatch").then((m) => ({
    default: m.BuyWatch,
  }));
  return buyWatchPromise;
}

export function loadMyFacts() {
  myFactsPromise ??= import("../features/portfolio/MyFacts").then((m) => ({
    default: m.MyFacts,
  }));
  return myFactsPromise;
}

/** Warm the initial route chunk while credentials or snapshot are in flight. */
export function preloadInitialRoute() {
  const view = initialView();
  if (view === "watch") void loadBuyWatch();
  else if (view === "facts") void loadMyFacts();
}
