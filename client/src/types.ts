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

export interface PortfolioCheckInInput {
  periodLabel: string;
  externalCashFlow: number;
  note: string;
  resetBaseline: boolean;
  useLedgerCashFlows: boolean;
}

export type PortfolioEventType = "deposit" | "withdrawal" | "dividend" | "interest" | "fee" | "tax" | "buy" | "sell";

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
  frozenThrough: string;
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

export interface PortfolioCheckInRecord {
  id: string;
  periodLabel: string;
  externalCashFlow: number;
  note: string;
  totalValue: number;
  previousCheckInId: string | null;
  previousTotalValue: number | null;
  totalChange: number | null;
  valuationResidual: number | null;
  baseCurrency: string;
  valuationDate: string | null;
  cashFlowSource: "baseline" | "ledger" | "manual";
  eventIds: string[];
  income: number;
  costs: number;
  turnover: number;
  modifiedDietzReturnPct: number | null;
  holdings: Holding[];
  allocationChanges: PortfolioAllocationChange[];
  createdAt: string;
}

export interface RiskFinding {
  level: "high" | "medium" | "low";
  title: string;
  detail: string;
  action: string;
}

export interface Snapshot {
  profile: FinancialProfile;
  goals: Goal[];
  holdings: Holding[];
  findings: RiskFinding[];
  totalValue: number;
  emergencyMonths: number;
  concentrationPct: number;
  valuationStatus: PortfolioValuationStatus;
  plan: PortfolioPlan;
  updatedAt: string;
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
  provider: "openai-compatible";
  baseUrl: string;
  model: string;
  hasApiKey: boolean;
}

export interface ModelConnectionTest {
  ok: boolean;
  model: string;
  latencyMs: number;
}

export interface CloudConfig {
  url: string;
  publishableKey: string;
}

export interface CloudStatus {
  configured: boolean;
  signedIn: boolean;
  email?: string;
  emailConfirmationPending: boolean;
  hasRecoveryKey: boolean;
  baseRevision: number;
  localChangedSinceSync: boolean;
  lastSyncedAt?: string;
  privacyBoundary: string[];
}

export interface AccountResult {
  signedIn: boolean;
  email: string;
  emailConfirmationPending: boolean;
  message: string;
}

export interface SyncResult {
  direction: "push" | "pull";
  revision: number;
  contentHash: string;
  recordCount: number;
  syncedAt: string;
  message: string;
}

export interface RecoveryKeyResult {
  recoveryKey: string;
  warning: string;
}

export interface AnalysisRequest {
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
  includePortfolioCheckins: boolean;
  includePortfolioEvents: boolean;
  includeEvidence: boolean;
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

export interface AnalysisWorkflowTrace {
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
}

export interface AnalysisOption {
  name: string;
  suitableWhen: string;
  tradeoffs: string[];
  risks: string[];
}

export interface AnalysisAction {
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

export interface DecisionEntry {
  id?: string;
  sourceAnalysisId?: string;
  sourceActionIndex?: number;
  assetName: string;
  thesis: string;
  counterThesis: string;
  expectedReturnPct: number;
  downsidePct: number;
  confidencePct: number;
  positionPct: number;
  invalidation: string;
  reviewDate: string;
  ruleChecks?: DecisionRuleCheck[];
}

export type RuleCheckStatus = "待确认" | "遵守" | "偏离" | "不适用";

export interface DecisionRuleCheck {
  ruleId: string;
  ruleRevision: number;
  category: string;
  statement: string;
  trigger: string;
  status: RuleCheckStatus;
  note: string;
}

export interface DecisionReview {
  outcomeSummary: string;
  actualReturnPct?: number;
  thesisStatus: "成立" | "部分成立" | "失效" | "尚不明确";
  processRating: number;
  lessons: string;
  reviewedAt?: string;
}

export interface DecisionRecord extends Omit<DecisionEntry, "id"> {
  id: string;
  ruleChecks: DecisionRuleCheck[];
  createdAt: string;
  review?: DecisionReview;
}

export interface ReminderSettings {
  enabled: boolean;
}

export interface ReviewReminderSummary {
  enabled: boolean;
  dueDecisionCount: number;
  periodicReviewDue: boolean;
  fingerprint: string;
  shouldNotify: boolean;
  checkedOn: string;
}

export interface RuleEffectivenessSummary {
  totalDecisions: number;
  evaluatedDecisions: number;
  applicableChecks: number;
  followedChecks: number;
  adherencePct: number | null;
  reviewedChecks: number;
  rules: RuleEffectivenessItem[];
}

export interface RuleEffectivenessItem {
  ruleId: string;
  currentRevision: number;
  observedRevisions: number[];
  category: string;
  statement: string;
  active: boolean;
  decisionCount: number;
  applicableCount: number;
  followedCount: number;
  deviatedCount: number;
  reviewedCount: number;
  followedProcessAverage: number | null;
  deviatedProcessAverage: number | null;
  signal: string;
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
  periodLabel: string;
  adherenceScore: number;
  processSummary: string;
  ruleViolations: string;
  lessons: string;
  nextActions: string;
  nextReviewDate: string;
}

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
  decisionTotal: number;
  reviewedDecisions: number;
  activeRules: number;
}

export interface SystemReviewRecord extends SystemReviewInput {
  id: string;
  snapshot: SystemReviewSnapshot;
  createdAt: string;
}

export interface ResearchEvidenceInput {
  assetName: string;
  title: string;
  publisher: string;
  sourceUrl: string;
  sourceTier: "一手来源" | "二手研究" | "媒体报道";
  evidenceType: "公司披露" | "监管文件" | "数据发布" | "研究报告" | "新闻" | "其他";
  stance: "支持" | "反驳" | "背景";
  asOfDate: string;
  claim: string;
  notes: string;
}

export interface ResearchEvidence extends ResearchEvidenceInput {
  id: string;
  active: boolean;
  capturedAt: string;
}
