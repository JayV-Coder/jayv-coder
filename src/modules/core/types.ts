/** O formato do que o núcleo Rust manda para a tela. Os nomes seguem o serde de
 * cada struct: quase tudo em camelCase; o retrato do sistema, em snake_case. */

export type TurnStatus = "queued" | "flying" | "answered" | "failed" | "blocked";
export type EntryVerdict = "pass" | "ask" | "block";
export type ExitVerdict = "cleared" | "held";
export type QuestionKind = "noul" | "single" | "multiple";
/** A cor do semáforo. É ela que pinta balão, cartão e lente. */
export type Aspect = "go" | "ask" | "stop";

export interface Project {
  id: string;
  name: string;
  rootPath: string;
  createdAt: string;
}

export interface Message {
  role: "user" | "assistant";
  content: string;
  createdAt: string;
  turnId?: string | null;
}

export interface Activity {
  at: string;
  seq: number;
  kind: string;
  detail: Record<string, unknown>;
}

export interface TurnView {
  id: string;
  code: string;
  status: TurnStatus;
  entry: EntryVerdict | null;
  exit: ExitVerdict | null;
  partial: string | null;
  activity: Activity[];
}

export interface Question {
  turnId: string;
  code: string;
  kind: QuestionKind;
  prompt: string;
  options: string[];
  source: string;
  status: string;
}

export interface Chat {
  id: string;
  code: string;
  projectId: string;
  title: string;
  messages: Message[];
  turns: TurnView[];
  question: Question | null;
  createdAt: string;
  updatedAt: string;
}

export interface WorkspaceData {
  projects: Project[];
  chats: Chat[];
}

export interface Turn {
  id: string;
  chatId: string;
  code: string;
  ordinal: number;
  status: TurnStatus;
  createdAt: string;
}

export interface Criterion {
  id: string;
  label: string;
  percent: number;
  band: [number, number] | null;
  reading: string;
  inverted: boolean;
}

export interface EntryCheck {
  id: string;
  at: string;
  chatId: string;
  turn: string;
  prompt: string;
  score: number;
  demand: number;
  verdict: EntryVerdict;
  scope: string;
  criteria: Criterion[];
  source: string;
  note: string;
}

export interface ExitCheck {
  id: string;
  at: string;
  chatId: string;
  turnId: string;
  turn: string;
  kind: string;
  target: string;
  rule: string | null;
  verdict: ExitVerdict;
}

export interface Tally {
  passed: number;
  asked: number;
  blocked: number;
  held: number;
}

export interface GateFeed {
  entries: EntryCheck[];
  exits: ExitCheck[];
  tally: Tally;
}

export interface SystemStatus {
  version: string;
  config_path: string;
  database_path: string;
  database_name: string;
  tables: { name: string; rows: number }[];
  providers: number;
  models: number;
  indexed_files: number;
  cache_entries: number;
  session_messages: number;
  performance_records: number;
}

export type AgentId = "claude" | "codex" | "copilot";
export type CostClass = "free" | "low" | "medium" | "high";
export type Speed = "fast" | "medium" | "slow";
export type Capability = "chat" | "code" | "reasoning" | "tools";

/** As opções de cada agente, com os valores que o núcleo aceita (ver
 * `src-tauri/src/llm.rs`). Nenhuma vira argumento cru: o núcleo monta a linha
 * de comando a partir delas. */
export interface ClaudeOptions {
  permissionMode: "default" | "plan" | "acceptEdits" | "auto" | "bypassPermissions";
  effort: "default" | "low" | "medium" | "high" | "xhigh" | "max";
  fallbackModel: string;
  maxBudgetUsd: number | null;
  blockedTools: string[];
  appendSystemPrompt: string;
  persistSessions: boolean;
  safeMode: boolean;
}

export interface CodexOptions {
  sandbox: "read-only" | "workspace-write" | "danger-full-access";
  reasoningEffort: "default" | "low" | "medium" | "high";
  networkAccess: boolean;
  skipGitRepoCheck: boolean;
}

export interface CopilotOptions {
  toolAccess: "read" | "edits" | "all";
  blockedTools: string[];
  silent: boolean;
}

export interface AgentOptions {
  claude: ClaudeOptions;
  codex: CodexOptions;
  copilot: CopilotOptions;
}

export interface AgentSettings<A extends AgentId = AgentId> {
  id: A;
  enabled: boolean;
  command: string;
  timeout: number;
  options: AgentOptions[A];
}

export interface AgentModel {
  agent: AgentId;
  model: string;
  enabled: boolean;
  capabilities: Capability[];
  costClass: CostClass;
  speed: Speed;
  contextWindow: number;
}

export interface LlmSettings {
  agents: AgentSettings[];
  models: AgentModel[];
}

export interface KnownModel {
  id: string;
  label: string;
  contextWindow: number;
  costClass: CostClass;
  speed: Speed;
}

export type Permission = "allow" | "ask" | "deny";
export const COMPLEXITIES = ["trivial", "simple", "medium", "complex"] as const;
export type Complexity = (typeof COMPLEXITIES)[number];

/** O que se ajusta no Jev e no app (ver `src-tauri/src/core_settings.rs`). */
export interface CoreSettings {
  adaptiveRouting: boolean;
  confidenceThreshold: number;
  preferLocal: boolean;
  budgets: Record<Complexity, number>;
  cacheTtl: number;
  exitRules: { read: Permission; write: Permission; shell: Permission };
  privacy: { deny: string[]; localOnly: string[]; redactSecrets: boolean };
}

/** Os números da portaria, que chegam do Supabase e só se leem aqui. */
export interface GateParameters {
  scopeDemand: [number, number, number];
  blockMargin: number;
  weights: Record<string, number>;
  scopeLevels: [string, string, string];
  noulLine: number;
}

export interface CoreSnapshot {
  settings: CoreSettings;
  defaults: CoreSettings;
  gate: GateParameters;
  confidenceRange: [number, number];
  budgetRange: [number, number];
  cacheTtlRange: [number, number];
  version: string;
}

export interface SettingsSnapshot {
  settings: LlmSettings;
  catalog: Record<AgentId, KnownModel[]>;
  timeoutRange: [number, number];
  contextRange: [number, number];
}

export interface AgentProbe {
  path: string | null;
  version: string | null;
}
