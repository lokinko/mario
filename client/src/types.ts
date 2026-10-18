export type RiskLevel = "保守" | "稳健" | "均衡" | "进取";

export interface FinancialProfile {
  exactAmounts?: {
    monthlyIncome: string;
    monthlyExpense: string;
    emergencyFund: string;
    liabilities: string;
    investableAssets: string;
  } | null;
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

export interface Holding {
  exactValues?: HoldingAmounts;
  id: string;
  symbol: string;
  name: string;
  assetClass: "现金" | "债券" | "股票" | "基金" | "黄金" | "其他" | "期权";
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

export interface OptionContract {
  underlyingId: string;
  account: string;
  optionType: "call" | "put";
  side: "long" | "short";
  strike: string;
  expiresOn: string;
  multiplier: string;
}
export interface StockOptionTotals {
  marketValue: string;
  costBasis: string;
  unrealizedProfit: string | null;
  settledProfit: string | null;
  optionSettledProfit: string;
  optionUnrealizedProfit: string;
  usable: boolean;
  openContracts: number;
}
export interface Snapshot {
  optionContracts?: Record<string, OptionContract>;
  stockOptionTotals?: Record<string, StockOptionTotals>;
  accounting?: {
    totalValue: string | null;
    holdings: Record<string, AccountingHolding>;
    investments: AccountingTotals;
    cash: AccountingTotals;
  } | null;
  corporateActions?: {
    id: string;
    holdingId: string;
    split: { effectiveOn: string; numerator: string; denominator: string };
    applied: boolean;
    dismissed?: boolean;
    userDeclared?: boolean;
    symbol?: string;
    sourceUrl: string;
  }[];
  cashBalances?: {
    currency: string;
    holdingId: string;
    amount: string;
    baselineId: string;
    confirmedOn: string;
    confirmedAt: string;
    needsReconciliation: boolean;
  }[];
  priceRefreshErrors?: string[];
  holdingTrades?: HoldingTrade[];
  holdingTradeSummaries?: Record<string, HoldingTradeSummary>;
  clearedTradeHoldings?: string[];
  holdingRevisions?: Record<string, string>;
  profile: FinancialProfile;
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

export interface HoldingAmounts {
  marketValue: string;
  costBasis: string;
  quantity: string | null;
  costPrice: string | null;
  currentPrice: string | null;
}
export interface AccountingHolding extends HoldingAmounts {
  priceStatus?: string;
  fxRate: string | null;
  unrealizedProfit: string | null;
  settledProfit: string | null;
  usable: boolean;
}
export type AccountingTotals = Record<
  "value" | "cost" | "unrealized" | "realized",
  { value: string | null; missing: number }
>;

export type HoldingTradeKind = "buy" | "sell" | "dividend" | "expire";
export interface HoldingTradeFields {
  settlementMode?: "history" | "settle";
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
  option?: OptionContract;
  name: string;
  symbol: string;
  assetClass: Holding["assetClass"];
  currency: string;
}
export interface HoldingTradeInput extends HoldingTradeFields {
  settlementMode: "history" | "settle";
  expectedCashBaseline?: string;
  requestId: string;
  holdingId: string;
  expectedRevision: string | null;
  newHolding: TradeSecurityInput | null;
}
export interface HoldingTradeUpdateInput extends HoldingTradeFields {
  settlementMode: "history" | "settle";
  expectedRevision: string;
}
export interface HoldingTradeDeleteInput {
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
  decisionReady?: boolean;
  baseCurrency: string;
  comparable: boolean;
  missingFxHoldings: string[];
  undatedHoldingCount: number;
  valuationDates: string[];
  alignedValuationDate: string | null;
  warnings: string[];
}

export interface PortfolioPlan {
  exactMonthlySurplus?: string | null;
  monthlySurplus: number;
  modeledAnnualReturnPct: number;
  modeledAnnualVolatilityPct: number;
  stressLossPct: number;
  riskCapacityPct: number;
  riskStatus: "within" | "near" | "over" | "insufficient";
  rebalancing: RebalanceAction[];
  assumptions: string;
}

export interface RebalanceAction {
  exactAmount?: string | null;
  holdingId: string;
  name: string;
  currentPct: number;
  targetPct: number;
  deviationPct: number;
  amount: number;
  direction: "增加" | "减少";
}

export type ModelProviderKind =
  "openai-responses" | "openai-chat" | "anthropic" | "codex-bridge";

export interface ModelConfig {
  provider: ModelProviderKind;
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
  reflect: boolean;
  exploreAlternatives: boolean;
  contextSelection: ContextSelection;
  previewRevision?: string;
}

export interface ContextSelection {
  includeProfile: boolean;
  includeHoldings: boolean;
  includePlanning: boolean;
  includeRiskFindings: boolean;
  includeRules: boolean;
  includeSystemReviews: boolean;
  includeDailyAssets: boolean;
  includeEvidence: boolean;
}

export interface AnalysisSettings {
  workflow: "quick" | "deep";
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
  provider: ModelProviderKind;
  model: string;
  workflow: "quick" | "deep";
  groups: ContextGroup[];
  evidenceCandidates: ResearchEvidence[];
  payload: Record<string, unknown>;
  payloadBytes: number;
  contextRevision: string;
  systemPolicy: string;
  localOnly: string[];
  generatedAt: string;
}

export interface AnalysisTransparency {
  provider: string;
  model: string;
  contextGroups: string[];
  payloadBytes: number;
  contextRevision: string;
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
  };
  version: string;
  researchPlan?: string;
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

export interface SystemReviewFields {
  content?: string | null;
  periodLabel: string;
  adherenceScore?: number | null;
  processSummary: string;
  ruleViolations: string;
  lessons: string;
  nextActions: string;
  nextReviewDate: string;
}

export type SystemReviewDraft = { content: string };

export interface SystemReviewSnapshot {
  portfolioValue: number;
  baseCurrency: string;
  portfolioComparable: boolean;
  emergencyMonths: number;
  concentrationPct: number;
  riskStatus: PortfolioPlan["riskStatus"];
  highRiskFindings: number;
  activeRules: number;
}

export interface SystemReviewRecord extends SystemReviewFields {
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
  state: "synced" | "signed_out" | "busy";
  sync?: { localUpdated?: boolean } | null;
}

export interface DailyAsset {
  accounting?: AccountingHolding | null;
  exactBaseValue?: string | null;
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
  exactTotalAssets?: string | null;
  exactLiabilities?: string;
  exactNetAssets?: string | null;
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
export interface FxBook {
  baseCurrency: string;
  checkedAt: number;
  requestedOn: string;
  quotes: FxRateQuote[];
  manual: Record<string, { rate: number; observedOn: string }>;
  error: string;
}
