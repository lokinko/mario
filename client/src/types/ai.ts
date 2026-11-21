import type {
  FinancialProfile,
  Holding,
  HoldingValuationEvidence,
  ResearchAttachment,
  ResearchEvidence,
} from "./user";

export type ModelProviderKind =
  "openai-responses" | "openai-chat" | "anthropic" | "codex-bridge";

export interface ModelConfig {
  provider: ModelProviderKind;
  baseUrl: string;
  model: string;
  hasApiKey: boolean;
  source?: "account" | "server";
  serverModel?: string;
}

export interface ModelConnectionTest {
  ok: boolean;
  model: string;
  latencyMs: number;
}

export interface PreviousTurn {
  question: string;
  answer: string;
  suggestions: string[];
  earlierQuestions?: string[];
  pendingQuestion?: string;
}

export interface AnalysisRequest {
  webSearch?: boolean;
  previousTurn?: PreviousTurn;
  question: string;
  workflow: "council";
  contextSelection: ContextSelection;
  previewRevision?: string;
}

export interface ContextSelection {
  includeProfile: boolean;
  includeHoldings: boolean;
  includeEvidence: boolean;
  includeMemory: boolean;
}

export interface AnalysisSettings {
  workflow: "council";
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
  workflow: "council";
  groups: ContextGroup[];
  evidenceCandidates: ResearchEvidence[];
  payload: Record<string, unknown>;
  renderedContext?: string;
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
  inputTokens?: number | null;
  outputTokens?: number | null;
  structuredOutputValidated: boolean;
  outputRepairs: number;
  memoryNotesUsed?: number;
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

/** Drafts are unvalidated live text of one proposal agent, keyed by its ID. */
export type AnalysisUpdate =
  | { type: "draft"; proposal: string; answer: string }
  | { type: "status"; message: string };

export type AnalysisEvent =
  | AnalysisUpdate
  | { type: "complete"; result: AnalysisResult }
  | { type: "error"; message: string };

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

export interface WebLead {
  title: string;
  url: string;
  summary: string;
}

export type ExecutionMode = "council" | "fast";

export interface RouteTrace {
  domain: string;
  domainLabel: string;
  executionMode: ExecutionMode;
  reason: string;
  companies?: string[];
}

export interface AnalysisWorkflowTrace {
  valuationEvidence?: HoldingValuationEvidence[];
  webSearch?: {
    status: "completed" | "incomplete" | "failed" | string;
    searchCalls: number;
    warnings: string[];
    leads?: WebLead[];
  } | null;
  /** Only present on historical analyses produced before the council workflow. */
  adviceGrounding?: AdviceGrounding[];
  personalContext?: {
    currentUserMessage?: string; // Read-only historical reference.
    question?: string;
    previousTurn?: Pick<PreviousTurn, "question" | "earlierQuestions"> &
      Partial<Omit<PreviousTurn, "question" | "earlierQuestions">>;
    discussionGuide?: unknown;
    financialProfile?: { profile: FinancialProfile; emergencyMonths: number };
    portfolio?: {
      holdings: Holding[];
      holdingValuations: HoldingValuationEvidence[];
    };
  } | null;
  version: string;
  calls: ModelCallTrace[];
  structuredReport?: StructuredAnalysis | LegacyStructuredAnalysis | null;
  outputValidation?: OutputValidationTrace | null;
  evidenceCatalog: AnalysisEvidenceReference[];
  route?: RouteTrace | null;
  research?: ResearchTrace | null;
  proposals?: ProposalTrace[];
  vote?: VoteTrace | null;
}

export interface ResearchTrace {
  brief?: {
    findings: AnalysisClaim[];
    counterpoints: string[];
    unknowns: string[];
    checks: {
      index: number;
      status: "covered" | "not_applicable" | "missing_source";
      note: string;
    }[];
  } | null;
  validation?: OutputValidationTrace | null;
  failure?: string | null;
}

export interface ProposalTrace {
  id: string;
  lens: string;
  label: string;
  report?: StructuredAnalysis | null;
  validation?: OutputValidationTrace | null;
  failure?: string | null;
}

export interface BallotAssessment {
  proposal: string;
  scores: {
    facts: number;
    citations: number;
    reasoning: number;
    counterpoints: number;
    actionability: number;
  };
  veto: string | null;
  objection: string;
}

export interface BallotTrace {
  voter: string;
  lens: string;
  ballot?: { preferred: string | null; assessments: BallotAssessment[] } | null;
  validation?: OutputValidationTrace | null;
  failure?: string | null;
}

export interface VoteTrace {
  status: "voted" | "tied" | "contested" | "single" | "unvoted";
  winner: string;
  tally: {
    proposal: string;
    voters: number;
    preferences: number;
    vetoes: number;
    scoreTotal: number;
    eliminated: boolean;
    rank: number;
  }[];
  ballots: BallotTrace[];
}

export type MemoryKind = "preference" | "viewpoint" | "context";

export type MemoryOrigin = "userStated" | "modelInferred";

export interface MemoryNote {
  id: string;
  kind: MemoryKind;
  origin: MemoryOrigin;
  statement: string;
  rationale: string;
  sourceAnalysisIds: string[];
  quote: string;
  active: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface DreamSummary {
  analyses: number;
  added: number;
  updated: number;
  archived: number;
  rejected: string[];
}

export interface MemoryOverview {
  items: MemoryNote[];
  dream: {
    dreamedThrough?: string | null;
    lastRunAt?: string | null;
    lastResult?: DreamSummary | null;
  };
  pendingAnalyses: number;
  dreamDue: boolean;
  enabled: boolean;
}

export interface StructuredAnalysis {
  answer: string;
  suggestions: AnalysisSuggestion[];
  facts: AnalysisClaim[];
  question: string | null;
}

export interface AnalysisSuggestion {
  action: string;
  why: string;
  factIndices: number[];
  unlessWhen: string;
}

export interface LegacyStructuredAnalysis {
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
  status: "valid" | "repaired" | "failed";
  attempts: number;
  errors: string[];
  /** The first output a repair replaced, kept for audit. */
  rejectedOutput?: string | null;
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

export interface ModelCallTrace {
  stage: string;
  label: string;
  latencyMs: number;
  inputTokens?: number;
  outputTokens?: number;
}
