import type { AnalysisSettings } from "../types";

export const analysisSettings: AnalysisSettings = {
  workflow: "quick",
  reflect: false,
  exploreAlternatives: false,
  webSearch: false,
  contextSelection: {
    includeProfile: true,
    includeHoldings: true,
    includeEvidence: true,
  },
};
