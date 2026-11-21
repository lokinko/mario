import { initialView, type View } from "./navigation";

let dailyReviewPromise: Promise<{
  default: typeof import("../features/dailyReview/DailyReviewPage").DailyReviewPage;
}> | null = null;
let buyWatchPromise: Promise<{
  default: typeof import("../features/watch/BuyWatch").BuyWatch;
}> | null = null;
let myFactsPromise: Promise<{
  default: typeof import("../features/portfolio/MyFacts").MyFacts;
}> | null = null;
let evidencePromise: Promise<{
  default: typeof import("../features/evidence/EvidenceWorkbench").EvidenceWorkbench;
}> | null = null;
let advisorPromise: Promise<{
  default: typeof import("../features/advisor/Advisor").Advisor;
}> | null = null;
let methodologyPromise: Promise<{
  default: typeof import("../features/methodology/InvestmentMethodology").InvestmentMethodology;
}> | null = null;
let modelSettingsPromise: Promise<{
  default: typeof import("../features/settings/ModelSettings").ModelSettings;
}> | null = null;
let serverDataPromise: Promise<{
  default: typeof import("../features/account/ServerData").ServerData;
}> | null = null;

export function loadDailyReview() {
  dailyReviewPromise ??= import("../features/dailyReview/DailyReviewPage").then(
    (m) => ({ default: m.DailyReviewPage }),
  );
  return dailyReviewPromise;
}

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

export function loadEvidenceWorkbench() {
  evidencePromise ??= import("../features/evidence/EvidenceWorkbench").then(
    (m) => ({ default: m.EvidenceWorkbench }),
  );
  return evidencePromise;
}

export function loadAdvisor() {
  advisorPromise ??= import("../features/advisor/Advisor").then((m) => ({
    default: m.Advisor,
  }));
  return advisorPromise;
}

export function loadInvestmentMethodology() {
  methodologyPromise ??=
    import("../features/methodology/InvestmentMethodology").then((m) => ({
      default: m.InvestmentMethodology,
    }));
  return methodologyPromise;
}

export function loadModelSettings() {
  modelSettingsPromise ??= import("../features/settings/ModelSettings").then(
    (m) => ({ default: m.ModelSettings }),
  );
  return modelSettingsPromise;
}

export function loadServerData() {
  serverDataPromise ??= import("../features/account/ServerData").then((m) => ({
    default: m.ServerData,
  }));
  return serverDataPromise;
}

/** Preload a route chunk on hover/focus or startup to remove tab switch latency. */
export function preloadView(view: View) {
  switch (view) {
    case "dailyReview":
      void loadDailyReview();
      break;
    case "watch":
      void loadBuyWatch();
      break;
    case "facts":
      void loadMyFacts();
      break;
    case "evidence":
      void loadEvidenceWorkbench();
      break;
    case "advisor":
      void loadAdvisor();
      break;
    case "methodology":
      void loadInvestmentMethodology();
      break;
    case "settings":
      void loadModelSettings();
      void loadServerData();
      break;
  }
}

/** Warm the initial route chunk while credentials or snapshot are in flight. */
export function preloadInitialRoute() {
  preloadView(initialView());
}
