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
}

export interface AnalysisResult {
  id: string;
  answer: string;
  stages: string[];
  createdAt: string;
  disclaimer: string;
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
