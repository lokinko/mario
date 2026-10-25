import type { AnalysisSettings } from "../types";

export const analysisSettings: AnalysisSettings = {
  workflow: "quick",
  webSearch: false,
  contextSelection: {
    includeProfile: true,
    includeHoldings: true,
    includeEvidence: true,
  },
};
