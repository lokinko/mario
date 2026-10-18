import type { AnalysisSettings } from "../types";

export const analysisSettings: AnalysisSettings = {
  workflow: "deep",
  reflect: true,
  exploreAlternatives: true,
  webSearch: true,
  contextSelection: {
    includeProfile: true,
    includeHoldings: true,
    includePlanning: false,
    includeRiskFindings: false,
    includeRules: false,
    includeSystemReviews: false,
    includeDailyAssets: false,
    includeEvidence: true,
  },
};
