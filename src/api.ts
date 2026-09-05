import { invoke } from "@tauri-apps/api/core";
import type {
  AnalysisRequest,
  AnalysisResult,
  DecisionEntry,
  FinancialProfile,
  Goal,
  Holding,
  ModelConfig,
  Snapshot,
} from "./types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

const demoProfile: FinancialProfile = {
  monthlyIncome: 28000,
  monthlyExpense: 14500,
  emergencyFund: 90000,
  liabilities: 120000,
  investableAssets: 560000,
  horizonYears: 10,
  maxDrawdownPct: 25,
  riskLevel: "均衡",
};

const demoHoldings: Holding[] = [
  { id: "1", symbol: "CASH", name: "现金管理", assetClass: "现金", marketValue: 80000, costBasis: 80000, currency: "CNY" },
  { id: "2", symbol: "BOND", name: "中短债基金", assetClass: "债券", marketValue: 120000, costBasis: 118000, currency: "CNY" },
  { id: "3", symbol: "INDEX", name: "宽基指数基金", assetClass: "基金", marketValue: 250000, costBasis: 230000, currency: "CNY" },
  { id: "4", symbol: "STOCK", name: "主动研究仓", assetClass: "股票", marketValue: 110000, costBasis: 98000, currency: "CNY" },
];

const demoSnapshot: Snapshot = {
  profile: demoProfile,
  goals: [],
  holdings: demoHoldings,
  findings: [
    { level: "medium", title: "主动仓集中度需要复核", detail: "最大单一主动风险敞口约为总投资资产的 19.6%。", action: "确认该仓位与最大可承受永久损失相匹配。" },
    { level: "low", title: "应急资金较充足", detail: "当前应急资金可覆盖约 6.2 个月支出。", action: "重大支出变化后重新评估。" },
  ],
  totalValue: 560000,
  emergencyMonths: 6.2,
  concentrationPct: 44.6,
  updatedAt: new Date().toISOString(),
};

export async function getSnapshot(): Promise<Snapshot> {
  return isTauri() ? invoke("get_snapshot") : demoSnapshot;
}

export async function saveProfile(profile: FinancialProfile): Promise<Snapshot> {
  return isTauri() ? invoke("save_profile", { profile }) : { ...demoSnapshot, profile };
}

export async function saveHolding(holding: Omit<Holding, "id">): Promise<Snapshot> {
  return isTauri() ? invoke("save_holding", { holding }) : demoSnapshot;
}

export async function saveGoal(goal: Omit<Goal, "id">): Promise<Snapshot> {
  return isTauri() ? invoke("save_goal", { goal }) : demoSnapshot;
}

export async function getModelConfig(): Promise<ModelConfig> {
  return isTauri()
    ? invoke("get_model_config")
    : { provider: "openai-compatible", baseUrl: "https://api.openai.com/v1", model: "gpt-4.1-mini", hasApiKey: false };
}

export async function saveModelConfig(config: Omit<ModelConfig, "hasApiKey"> & { apiKey?: string }): Promise<ModelConfig> {
  return isTauri() ? invoke("save_model_config", { config }) : { ...config, hasApiKey: Boolean(config.apiKey) };
}

export async function runAnalysis(request: AnalysisRequest): Promise<AnalysisResult> {
  if (isTauri()) return invoke("run_analysis", { request });
  await new Promise((resolve) => setTimeout(resolve, 700));
  return {
    id: crypto.randomUUID(),
    answer: "当前为浏览器演示模式。安装桌面版并配置模型密钥后，系统会先执行确定性风险检查，再基于目标、期限、组合与决策记录进行多阶段分析。",
    stages: ["规则风险检查", "构建投资上下文", "生成分析", "反方审查"],
    createdAt: new Date().toISOString(),
    disclaimer: "本分析用于投资教育与决策支持，不构成收益保证或个性化投资建议。",
  };
}

export async function saveDecision(entry: DecisionEntry): Promise<void> {
  if (isTauri()) await invoke("save_decision", { entry });
}
