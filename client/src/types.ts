export type RiskLevel = "保守" | "稳健" | "均衡" | "进取";

export interface FinancialProfile {
  monthlyIncome: number;
  monthlyExpense: number;
  emergencyFund: number;
  liabilities: number;
  investableAssets: number;
  horizonYears: number;
  maxDrawdownPct: number;
  riskLevel: RiskLevel;
  baseCurrency: string;
}

export interface Goal {
  id: string;
  name: string;
  targetAmount: number;
  currentAmount: number;
  monthlyContribution: number;
  targetDate: string;
  priority: "刚性" | "重要" | "弹性";
}

export interface Holding {
  id: string;
  symbol: string;
  name: string;
  assetClass: "现金" | "债券" | "股票" | "基金" | "黄金" | "其他";
  marketValue: number;
  costBasis: number;
  costPrice?: number | null;
  quantity?: number | null;
  currentPrice?: number | null;
  targetPct: number;
  currency: string;
  fxRateToBase: number | null;
  valuationDate: string;
  fxRateSource: string;
  fxRateObservedOn: string;
}

export interface FxRateQuote {
  fromCurrency: string;
  toCurrency: string;
  rate: number;
  requestedOn: string;
  observedOn: string;
  stalenessDays: number;
  providerCode: string;
  providerName: string;
  sourceUrl: string;
  methodologyUrl: string;
  disclaimer: string;
}

export interface SecurityPriceQuote {
  symbol: string;
  currency: string;
  close: number;
  requestedOn: string;
  observedOn: string;
  stalenessDays: number;
  providerCode: string;
  providerName: string;
  exchange: string;
  micCode: string;
  instrumentType: string;
  priceBasis: string;
  sourceUrl: string;
  methodologyUrl: string;
  disclaimer: string;
}

export interface SecurityPriceConfig {
  provider: "twelve-data";
  hasApiKey: boolean;
}

export interface HoldingValuationEvidence {
  holdingId: string;
  symbol: string;
  quantity: number;
  unitPrice: number;
  marketValue: number;
  currency: string;
  requestedOn: string;
  observedOn: string;
  stalenessDays: number;
  providerCode: string;
  providerName: string;
  exchange: string;
  micCode: string;
  instrumentType: string;
  priceBasis: string;
  sourceUrl: string;
  methodologyUrl: string;
  disclaimer: string;
  capturedAt: string;
}

export type PortfolioEventType =
  | "deposit"
  | "withdrawal"
  | "dividend"
  | "interest"
  | "fee"
  | "tax"
  | "buy"
  | "sell";

export interface PortfolioEventInput {
  eventType: PortfolioEventType;
  source: string;
  externalId: string;
  assetName: string;
  amount: number;
  currency: string;
  fxRateToBase: number | null;
  fxRateSource: string;
  fxRateObservedOn: string;
  occurredOn: string;
  note: string;
}

export interface PortfolioEventRecord extends PortfolioEventInput {
  id: string;
  baseCurrency: string;
  baseAmount: number;
  reversalOfEventId: string | null;
  reversedByEventId: string | null;
  createdAt: string;
}

export interface PortfolioEventReversalInput {
  occurredOn: string;
  note: string;
}

export type PortfolioEventImportStatus = "ready" | "duplicate" | "error";

export interface PortfolioEventImportRow {
  rowNumber: number;
  status: PortfolioEventImportStatus;
  message: string;
  source: string;
  externalId: string;
  eventType: string;
  occurredOn: string;
  amount: number | null;
  currency: string;
  fxRateToBase: number | null;
  fxRateSource: string;
  fxRateObservedOn: string;
  assetName: string;
  note: string;
}

export interface PortfolioEventImportPreview {
  rows: PortfolioEventImportRow[];
  readyCount: number;
  duplicateCount: number;
  errorCount: number;
  previewRevision: string;
  baseCurrency: string;
}

export interface PortfolioEventImportResult {
  insertedCount: number;
  duplicateCount: number;
  previewRevision: string;
}

export interface PortfolioAllocationChange {
  assetClass: string;
  previousValue: number;
  currentValue: number;
  valueChange: number;
  previousPct: number;
  currentPct: number;
  pctPointChange: number;
}

export interface RiskFinding {
  level: "high" | "medium" | "low";
  title: string;
  detail: string;
  action: string;
}

export interface Snapshot {
  priceRefreshErrors?: string[];
  holdingTrades?: HoldingTrade[];
  holdingTradeSummaries?: Record<string, HoldingTradeSummary>;
  clearedTradeHoldings?: string[];
  holdingRevisions?: Record<string, string>;
  profile: FinancialProfile;
  goals: Goal[];
  holdings: Holding[];
  holdingValuations: HoldingValuationEvidence[];
  findings: RiskFinding[];
  totalValue: number;
  emergencyMonths: number;
  concentrationPct: number;
  valuationStatus: PortfolioValuationStatus;
  plan: PortfolioPlan;
  updatedAt: string;
}

export type HoldingTradeKind = "buy" | "sell" | "dividend";
export interface HoldingTradeFields {
  kind: HoldingTradeKind;
  occurredOn: string;
  sequence: number;
  quantity: string;
  price: string;
  grossAmount: string;
  fees: string;
  taxes: string;
  account: string;
  externalId: string;
  note: string;
}
export interface HoldingTrade extends HoldingTradeFields {
  id: string;
  holdingId: string;
  currency: string;
  createdAt: string;
  updatedAt: string;
}
export interface TradeSecurityInput {
  name: string;
  symbol: string;
  assetClass: Holding["assetClass"];
  currency: string;
}
export interface HoldingTradeInput extends HoldingTradeFields {
  requestId: string;
  holdingId: string;
  expectedRevision: string | null;
  newHolding: TradeSecurityInput | null;
}
export interface HoldingTradeUpdateInput extends HoldingTradeFields {
  expectedRevision: string;
}
export interface HoldingTradeSummary {
  account: string;
  quantity: string;
  costBasis: string;
  averageCost: string;
  realizedProfit: string;
  dividendIncome: string;
  settledProfit: string;
  totalFees: string;
  totalTaxes: string;
  netCashFlow: string;
  profits: Record<string, string>;
  cashFlows: Record<string, string>;
}

export interface PortfolioValuationStatus {
  baseCurrency: string;
  comparable: boolean;
  missingFxHoldings: string[];
  undatedHoldingCount: number;
  valuationDates: string[];
  alignedValuationDate: string | null;
  warnings: string[];
}

export interface PortfolioPlan {
  monthlySurplus: number;
  committedMonthly: number;
  modeledAnnualReturnPct: number;
  modeledAnnualVolatilityPct: number;
  stressLossPct: number;
  riskCapacityPct: number;
  riskStatus: "within" | "near" | "over" | "insufficient";
  goalProjections: GoalProjection[];
  rebalancing: RebalanceAction[];
  assumptions: string;
}

export interface GoalProjection {
  goalId: string;
  name: string;
  monthsRemaining: number;
  fundedPct: number;
  estimatedSuccessPct: number;
  conservativeAmount: number;
  medianAmount: number;
  requiredMonthlyContribution: number;
  monthlyGap: number;
  status: "on-track" | "watch" | "off-track" | "reached" | "expired";
}

export interface RebalanceAction {
  holdingId: string;
  name: string;
  currentPct: number;
  targetPct: number;
  deviationPct: number;
  amount: number;
  direction: "增加" | "减少";
}

export interface ModelConfig {
  provider:
    | "openai-responses"
    | "openai-chat"
    | "anthropic"
    | "openai-compatible"
    | "codex";
  baseUrl: string;
  model: string;
  hasApiKey: boolean;
}

export interface ModelConnectionTest {
  ok: boolean;
  model: string;
  latencyMs: number;
}

export interface AnalysisRequest {
  dailyAssetRange?: { from: string; to: string };
  webSearch?: boolean;
  userMessage?: string;
  question: string;
  workflow: "quick" | "deep";
  useMemory: boolean;
  reflect: boolean;
  exploreAlternatives: boolean;
  excludedMemoryIds: string[];
  contextSelection: ContextSelection;
  previewRevision?: string;
}

export interface ContextSelection {
  includeProfile: boolean;
  includeGoals: boolean;
  includeHoldings: boolean;
  includePlanning: boolean;
  includeRiskFindings: boolean;
  includeRules: boolean;
  includeSystemReviews: boolean;
  includeDailyAssets: boolean;
  includePortfolioEvents: boolean;
  includeEvidence: boolean;
}

export interface AnalysisSettings {
  workflow: "quick" | "deep";
  useMemory: boolean;
  reflect: boolean;
  exploreAlternatives: boolean;
  webSearch: boolean;
  contextSelection: ContextSelection;
}

export interface ContextGroup {
  key: string;
  label: string;
  included: boolean;
  recordCount: number;
  sensitivity: string;
  description: string;
}

export interface AnalysisPreview {
  provider: string;
  model: string;
  workflow: "quick" | "deep";
  groups: ContextGroup[];
  memoryCandidates: MemoryCandidate[];
  evidenceCandidates: ResearchEvidence[];
  payload: Record<string, unknown>;
  payloadBytes: number;
  contextRevision: string;
  memoryPolicy: string;
  systemPolicy: string;
  localOnly: string[];
  generatedAt: string;
}

export interface MemoryCandidate {
  id: string;
  kind: string;
  title: string;
  summary: string;
  content: Record<string, unknown>;
  createdAt: string;
  occurredAt: string;
  status: string;
  reviewed: boolean;
  contradiction: boolean;
  tags: string[];
  preference: "default" | "pinned" | "hidden";
  preferenceNote: string;
  preferenceUpdatedAt: string | null;
  selected: boolean;
  retrieval?: MemoryRetrieval;
}

export interface MemoryPreferenceInput {
  preference: "default" | "pinned" | "hidden";
  note: string;
}

export interface MemoryRetrieval {
  score: number;
  ageDays: number;
  reasons: string[];
  passes: string[];
}

export interface AnalysisTransparency {
  provider: string;
  model: string;
  contextGroups: string[];
  payloadBytes: number;
  contextRevision: string;
  memoryItemsUsed: number;
  reviewedMemoryItemsUsed: number;
  conflictingMemoryItemsUsed: number;
  evidenceItemsUsed: number;
  citationsRequired: boolean;
  modelCalls: number;
  totalLatencyMs: number;
  inputTokens?: number;
  outputTokens?: number;
  structuredOutputValidated: boolean;
  outputRepairs: number;
  externalDataUsed: boolean;
  apiKeySent: boolean;
}

export interface AnalysisResult {
  id: string;
  answer: string;
  stages: string[];
  transparency: AnalysisTransparency;
  workflowTrace: AnalysisWorkflowTrace;
  createdAt: string;
  disclaimer: string;
}

export interface AnalysisHistoryItem {
  groundingStatus?:
    "supported" | "insufficient" | "contradicted" | "unavailable";
  id: string;
  question: string;
  createdAt: string;
  transparency?: AnalysisTransparency;
  workflowVersion?: string;
  verdict?: string;
}

export interface StoredAnalysis {
  id: string;
  question: string;
  answer: string;
  createdAt: string;
  transparency?: AnalysisTransparency;
  workflowTrace?: AnalysisWorkflowTrace;
}

export interface AdviceGrounding {
  actionIndex: number;
  status: "supported" | "insufficient" | "contradicted" | "unavailable";
  reason: string;
  checks: {
    factIndex: number;
    sourceKind: string;
    sourceRef: string;
    quote: string;
  }[];
}
export interface AnalysisWorkflowTrace {
  webSearch?: { status: string; searchCalls: number; warnings: string[] };
  adviceGrounding?: AdviceGrounding[];
  personalContext?: {
    currentUserMessage?: string;
    financialProfile?: { profile: FinancialProfile; emergencyMonths: number };
    portfolio?: {
      holdings: Holding[];
      holdingValuations: HoldingValuationEvidence[];
    };
    goals?: Goal[];
  };
  version: string;
  researchPlan?: string;
  memoryItems: MemoryCandidate[];
  alternatives: AnalysisAlternative[];
  critique?: string;
  calls: ModelCallTrace[];
  structuredReport?: StructuredAnalysis;
  outputValidation?: OutputValidationTrace;
  evidenceCatalog: AnalysisEvidenceReference[];
}

export interface StructuredAnalysis {
  verdict: string;
  facts: AnalysisClaim[];
  inferences: AnalysisClaim[];
  unknowns: string[];
  options: AnalysisOption[];
  actions: AnalysisAction[];
  reviewTriggers: string[];
}

export interface AnalysisClaim {
  statement: string;
  basis: "user_data" | "research_evidence";
  evidenceIds: string[];
  sourceRefs?: string[];
}

export interface AnalysisOption {
  name: string;
  suitableWhen: string;
  tradeoffs: string[];
  risks: string[];
}

export interface AnalysisAction {
  supportingFactIndices?: number[];
  evidenceLimits?: string[];
  action: string;
  rationale: string;
  reversible: boolean;
  reviewTrigger: string;
}

export interface OutputValidationTrace {
  status: "valid" | "repaired";
  attempts: number;
  errors: string[];
}

export interface AnalysisEvidenceReference {
  attachments?: ResearchAttachment[];
  assetName?: string;
  evidenceType?: string;
  claim?: string;
  notes?: string;
  stance?: string;
  capturedAt?: string;
  id: string;
  title: string;
  publisher: string;
  sourceUrl: string;
  sourceTier: string;
  asOfDate: string;
}

export interface AnalysisAlternative {
  id: string;
  label: string;
  lens: string;
  content: string;
}

export interface ModelCallTrace {
  stage: string;
  label: string;
  latencyMs: number;
  inputTokens?: number;
  outputTokens?: number;
}

export interface ReviewReminderSummary {
  periodicReviewDue: boolean;
  checkedOn: string;
}

export interface InvestmentRuleInput {
  category: "资产配置" | "风险" | "研究" | "仓位" | "行为" | "复盘";
  statement: string;
  trigger: string;
  rationale: string;
  active: boolean;
  sourceReviewId?: string;
}

export interface InvestmentRule extends InvestmentRuleInput {
  id: string;
  revision: number;
  createdAt: string;
  updatedAt: string;
}

export interface InvestmentRuleRevision extends InvestmentRuleInput {
  ruleId: string;
  revision: number;
  changedAt: string;
}

export interface SystemReviewInput {
  content?: string | null;
  periodLabel: string;
  adherenceScore?: number | null;
  processSummary: string;
  ruleViolations: string;
  lessons: string;
  nextActions: string;
  nextReviewDate: string;
}

export type SystemReviewDraft = { content: string } | SystemReviewInput;

export interface SystemReviewSnapshot {
  portfolioValue: number;
  baseCurrency: string;
  portfolioComparable: boolean;
  emergencyMonths: number;
  concentrationPct: number;
  riskStatus: PortfolioPlan["riskStatus"];
  highRiskFindings: number;
  goalTotal: number;
  goalsOnTrack: number;
  activeRules: number;
}

export interface SystemReviewRecord extends SystemReviewInput {
  id: string;
  snapshot: SystemReviewSnapshot;
  createdAt: string;
}

export type ResearchEvidenceInput = {
  content: string;
  attachments?: { name: string; dataBase64: string }[];
};

export interface ResearchAttachment {
  id: string;
  name: string;
  byteSize: number;
  extractionNote: string;
}

export interface ResearchEvidenceFields {
  assetName: string;
  title: string;
  publisher: string;
  sourceUrl: string;
  sourceTier: "一手来源" | "二手研究" | "媒体报道" | "用户资料（待核实）";
  evidenceType:
    | "公司披露"
    | "监管文件"
    | "数据发布"
    | "研究报告"
    | "新闻"
    | "其他"
    | "user_source_text";
  stance: "支持" | "反驳" | "背景";
  asOfDate: string;
  claim: string;
  notes: string;
}

export interface ResearchEvidence extends ResearchEvidenceFields {
  attachments?: ResearchAttachment[];
  id: string;
  active: boolean;
  capturedAt: string;
}

export interface AutoSyncResult {
  state: "synced" | "disabled" | "signed_out" | "busy";
  sync?: { localUpdated?: boolean } | null;
}

export interface DailyAsset {
  holding: Holding;
  confirmedOn: string | null;
  carried: boolean;
  removed: boolean;
  baseValue: number | null;
  previousDayChange?: number | null;
  pctPointChange?: number | null;
  changeKind?: string;
}
export interface DailyRecord {
  day: string;
  baseCurrency: string;
  totalAssets: number | null;
  liabilities: number;
  netAssets: number | null;
  carried: boolean;
  assets: DailyAsset[];
  missingFx: string[];
  previousDayChange: number | null;
  lastUpdatedDay: string | null;
  sinceLastUpdateChange: number | null;
}
export interface DailyHistory {
  timezone: string;
  today: string;
  records: DailyRecord[];
  nextBefore: string | null;
}
export interface DailyComparison {
  from: DailyRecord;
  to: DailyRecord;
  amountChange: number | null;
  assets: {
    id: string;
    name: string;
    status: "added" | "removed" | "existing";
    previous: DailyAsset | null;
    current: DailyAsset | null;
    amountChange: number | null;
    pctPointChange: number | null;
  }[];
  allocationChanges: PortfolioAllocationChange[];
}

export interface FxBook {
  baseCurrency: string;
  checkedAt: number;
  requestedOn: string;
  quotes: FxRateQuote[];
  manual: Record<string, { rate: number; observedOn: string }>;
  error: string;
}
