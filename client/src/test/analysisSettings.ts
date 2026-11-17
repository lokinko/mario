import type { AnalysisSettings } from "../types/ai";

export const analysisSettings: AnalysisSettings = {
  workflow: "council",
  webSearch: false,
  contextSelection: {
    includeProfile: true,
    includeHoldings: true,
    includeEvidence: true,
    includeMemory: true,
  },
};
