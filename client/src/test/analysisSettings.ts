import type { AnalysisSettings } from "../types";

export const analysisSettings: AnalysisSettings = {
  workflow: "deep",
  useMemory: true,
  reflect: true,
  exploreAlternatives: true,
  webSearch: true,
  contextSelection: {
    includeProfile: true,
    includeGoals: true,
    includeHoldings: true,
    includePlanning: true,
    includeRiskFindings: true,
    includeRules: true,
    includeSystemReviews: true,
    includePortfolioCheckins: true,
    includePortfolioEvents: true,
    includeEvidence: true,
  },
};
