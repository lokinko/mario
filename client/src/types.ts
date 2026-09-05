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
  plan: PortfolioPlan;
  updatedAt: string;
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

export interface AnalysisRequest {
  question: string;
  workflow: "quick" | "deep";
  useMemory: boolean;
  reflect: boolean;
  exploreAlternatives: boolean;
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
  content: string;
  createdAt: string;
}

export interface AnalysisTransparency {
  provider: string;
  model: string;
  contextGroups: string[];
  payloadBytes: number;
  contextRevision: string;
  memoryItemsUsed: number;
  evidenceItemsUsed: number;
  citationsRequired: boolean;
  modelCalls: number;
  totalLatencyMs: number;
  inputTokens?: number;
  outputTokens?: number;
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

export interface AnalysisWorkflowTrace {
  version: string;
  researchPlan?: string;
  alternatives: AnalysisAlternative[];
  critique?: string;
  calls: ModelCallTrace[];
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
  assetName: string;
  thesis: string;
  counterThesis: string;
  expectedReturnPct: number;
  downsidePct: number;
  confidencePct: number;
  positionPct: number;
  invalidation: string;
  reviewDate: string;
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
  createdAt: string;
  review?: DecisionReview;
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
