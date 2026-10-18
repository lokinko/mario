import type { AnalysisSettings } from "../types";

export const analysisSettings: AnalysisSettings = {
  workflow: "deep",
  reflect: true,
  exploreAlternatives: true,
  webSearch: true,
  contextSelection: {
    includeProfile: true,
    includeHoldings: true,
    includePlanning: true,
    includeRiskFindings: true,
    includeRules: true,
    includeSystemReviews: true,
    includeDailyAssets: true,
    includeEvidence: true,
  },
};
