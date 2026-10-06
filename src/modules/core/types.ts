/** O formato do que o núcleo Rust manda para a tela. Os nomes seguem o serde de
 * cada struct: quase tudo em camelCase; o retrato do sistema, em snake_case. */

export type TurnStatus = "queued" | "flying" | "answered" | "failed" | "blocked";
export type EntryVerdict = "pass" | "ask" | "block";
export type ExitVerdict = "cleared" | "held";
export type QuestionKind = "noul" | "single" | "multiple" | "form";

/** Uma pergunta do formulário: vem gravada como texto JSON em `options`. */
export interface FormItem {
  prompt: string;
  options: string[];
}
/** A cor do semáforo. É ela que pinta balão, cartão e lente. */
export type Aspect = "go" | "ask" | "stop";

export interface Project {
  id: string;
  name: string;
  rootPath: string;
  createdAt: string;
  /** As chaves dos remotes da pasta (`github.com/acme/api`). */
  repoKeys: string[];
  /** A organização de que este projeto é o chat: a pasta dele junta os
   * repositórios dela. Nulo nos projetos de um repositório só. */
  orgId?: string | null;
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
  /** Quem atendeu: o agente (CLI), o modelo, o modo e o papel que o Jev deu. */
  route: TurnRoute | null;
}

/** O que sustenta a resposta de um turno, lido sob demanda. As portarias são
 * o que o JayV observou; a segunda opinião é a leitura de outro modelo. */
export interface TurnEvidence {
  entry: { score: number; demand: number; verdict: EntryVerdict; scopeLevel: number; doneCriterion: boolean } | null;
  exits: { kind: string; target: string; rule: string | null; verdict: ExitVerdict }[];
  /** `arrived` é falso enquanto a revisão não chegou ou quando o revisor falhou. */
  review: { provider: string; model: string; files: number; arrived: boolean } | null;
}

export type RouteMode = "plan" | "build";

export interface TurnRoute {
  provider: string;
  model: string;
  /** Vazio nos turnos narrados antes de o modo existir. */
  mode: RouteMode | null;
  agent: string | null;
  switched?: ModeSwitch | null;
}

/** O que o desenvolvedor libera só para o próximo pedido, por cima das
 * configurações dos agentes. `commands` são comandos aprovados no chat. */
export interface Grants {
  shell: boolean;
  git: boolean;
  network: boolean;
  commands: string[];
}

/** Um servidor MCP que o JayV entrega aos agentes. `stdio` sobe `command`;
 * `http` fala com `url`. `agents` vazio é todos os que sabem receber. */
export interface McpServer {
  name: string;
  transport: "stdio" | "http";
  command: string;
  args: string[];
  env: Record<string, string>;
  url: string;
  headers: Record<string, string>;
  enabled: boolean;
  agents: string[];
}

/** Uma skill instalada: a pasta com `SKILL.md` que o Jev pode escolher. */
export interface Skill {
  name: string;
  description: string;
  path: string;
  enabled: boolean;
  installedAt: string;
}

/** O que as organizações de quem usa dão (servidores MCP e skills), como a
 * sincronização trouxe. Só leitura, e sem segredos: de um servidor vêm o
 * comando (ou o endereço sem caminho) e os agentes. */
export interface OrgExtensions {
  mcp: { org: string; name: string; transport: "stdio" | "http"; command: string; url: string; agents: string[] }[];
  skills: { org: string; name: string; description: string }[];
}

export interface McpDraft {
  servers: McpServer[];
  /** Montado por um modelo a partir de uma descrição, não lido do texto. */
  fromModel: boolean;
}

export const NO_GRANTS: Grants = { shell: false, git: false, network: false, commands: [] };

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
  /** O modo fixado no chat: `auto` deixa o Jev escolher a cada pedido. */
  workMode: WorkMode;
  /** O último pedido escrito, para o cartão: vem mesmo quando `messages`
   * chega vazio (o retrato leve de `get_workspace`). */
  lastPrompt?: string | null;
  /** Quantas mensagens o chat tem. Maior que `messages.length`: a conversa
   * ainda não foi lida (`refreshChat`). */
  messageCount?: number;
}

/** O modo de trabalho do chat: o Jev escolhe, só planejamento ou desenvolvimento. */
export type WorkMode = "auto" | RouteMode;
export const WORK_MODES: WorkMode[] = ["auto", "plan", "build"];

/** O Jev tirou o chat do modo em que ele estava: de onde e por quê. */
export interface ModeSwitch {
  from: "auto" | "plan";
  /** `asked`: estava em planejamento e o pedido é para implementar;
   * `repeated`: no automático, o pedido para implementar veio de novo. */
  reason: "asked" | "repeated";
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

export type AgentId = "claude" | "codex" | "copilot" | "cursor";
export type CostClass = "free" | "low" | "medium" | "high";
export type Speed = "fast" | "medium" | "slow";
export type Capability = "chat" | "code" | "reasoning" | "tools";

/** As opções de cada agente, com os valores que o núcleo aceita (ver
 * `src-tauri/crates/jayv-agents/src/llm.rs`). Nenhuma vira argumento cru: o núcleo monta a linha
 * de comando a partir delas. */
export interface ClaudeOptions {
  /** `default` é o nome antigo do `manual`, gravado antes da v0.60.0. */
  permissionMode: "manual" | "default" | "plan" | "acceptEdits" | "auto" | "dontAsk" | "bypassPermissions";
  effort: "auto" | "low" | "medium" | "high" | "xhigh" | "max";
  fallbackModel: string;
  maxBudgetUsd: number | null;
  blockedTools: string[];
  appendSystemPrompt: string;
  persistSessions: boolean;
  safeMode: boolean;
  /** As ferramentas do índice de símbolos do JayV (`jayv mcp`). */
  symbolTools: boolean;
  /** Os mecanismos liberados sem pergunta (`AGENT_MECHANISMS.claude`). */
  mechanisms: Mechanism[];
}

export interface CodexOptions {
  sandbox: "read-only" | "workspace-write" | "danger-full-access";
  reasoningEffort: "auto" | "low" | "medium" | "high";
  networkAccess: boolean;
  skipGitRepoCheck: boolean;
  mechanisms: Mechanism[];
}

export interface CopilotOptions {
  toolAccess: "read" | "edits" | "all";
  blockedTools: string[];
  silent: boolean;
  mechanisms: Mechanism[];
}

export interface CursorOptions {
  sandbox: "default" | "enabled" | "disabled";
  force: boolean;
  approveMcps: boolean;
}

/** O que o agente pode fazer além de ler e editar o projeto: buscar na web,
 * abrir páginas, rodar comandos, usar as ferramentas do GitHub. */
export type Mechanism = "webSearch" | "webFetch" | "shell" | "githubTools";

/** Os mecanismos que a CLI de cada agente sabe ligar por flag (o mesmo
 * `mechanisms_of` do núcleo). O Cursor não tem flag para nenhum. */
export const AGENT_MECHANISMS: Record<AgentId, Mechanism[]> = {
  claude: ["webSearch", "webFetch", "shell"],
  codex: ["webSearch"],
  copilot: ["webFetch", "shell", "githubTools"],
  cursor: [],
};

export interface AgentOptions {
  claude: ClaudeOptions;
  codex: CodexOptions;
  copilot: CopilotOptions;
  cursor: CursorOptions;
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
  capabilities: Capability[];
}

export type Permission = "allow" | "ask" | "deny";
export const COMPLEXITIES = ["trivial", "simple", "medium", "complex"] as const;
export type Complexity = (typeof COMPLEXITIES)[number];

/** O que se ajusta no Jev e no app (ver `src-tauri/crates/jayv-jev/src/core_settings.rs`). */
export interface CoreSettings {
  adaptiveRouting: boolean;
  confidenceThreshold: number;
  preferLocal: boolean;
  budgets: Record<Complexity, number>;
  cacheTtl: number;
  exitRules: { read: Permission; write: Permission; shell: Permission };
  privacy: { deny: string[]; localOnly: string[]; redactSecrets: boolean };
  /** Os agentes na ordem de preferência para desempatar; vazia, os empatados
   * se espalham entre os chats. */
  agentOrder: AgentId[];
  /** Um agente de outro provedor revisa o que o modo build mudou. */
  reviewChanges: boolean;
  /** Num pedido complexo do build, um modelo de raciocínio planeja antes. */
  planFirst: boolean;
  /** Num pedido complexo do build, partes vão a agentes diferentes ao mesmo tempo. */
  parallelTasks: boolean;
  /** Num chat com sessão viva, o modelo da sessão fica (padrão ligado). */
  keepSessionModel: boolean;
  /** A sessão do agente atravessa a troca entre planejamento e build. */
  resumeAcrossModes: boolean;
  /** O teto total de um pedido, em minutos: passou, o pedido para e os
   * agentes abertos caem junto. */
  turnCeilingMinutes: number;
}

/** Os números da portaria, que chegam do Supabase e só se leem aqui. */
export interface GateParameters {
  scopeDemand: [number, number, number];
  blockMargin: number;
  weights: Record<string, number>;
  scopeLevels: [string, string, string];
  noulLine: number;
}

export type Expertise = "starter" | "junior" | "mid" | "senior" | "architect";
export const EXPERTISE_LEVELS: Expertise[] = ["starter", "junior", "mid", "senior", "architect"];

/** O que um nível faz com a portaria e com o Jev. */
export interface LevelView {
  id: Expertise;
  scopeDemand: [number, number, number];
  blockMargin: number;
  confidence: number;
  buildCeiling: Complexity;
  destructiveThreshold: number;
}

export interface CoreSnapshot {
  settings: CoreSettings;
  defaults: CoreSettings;
  /** Os números da portaria já no nível da conta. */
  gate: GateParameters;
  expertise: Expertise;
  levels: LevelView[];
  /** O nível que o histórico da portaria sugere, ou nada. */
  suggestion: LevelSuggestion | null;
  /** A regra de código enxuto no modo build. */
  leanCode: boolean;
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

/** A lista de um agente lida de novo do `/model` do CLI. `listed` é falso
 * quando o CLI não respondeu e nada mudou. */
export interface ModelsRefresh { snapshot: SettingsSnapshot; listed: boolean }

export interface AgentProbe {
  path: string | null;
  version: string | null;
  /** Só na conferência pedida, e só no agente que sabe dizer (Claude, Codex). */
  loggedIn?: boolean;
}

/** De quem é a conta das estatísticas (ver `src-tauri/crates/jayv-agents/src/usage/store.rs`). */
/** De quem é a conta. `projects` é um punhado de projetos (os de uma
 * organização); vazio, não conta nada. */
export type UsageScope = { kind: "global" } | { kind: "project"; id: string } | { kind: "projects"; id: string[] } | { kind: "chat"; id: string };
/** De onde vem um número: a ferramenta informou, o app calculou, ou é de um
 * turno antigo, de antes da contagem existir. */
export type UsagePrecision = "reported" | "estimated" | "legacy";

export interface UsageTotals {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  /** Só o que as ferramentas informaram; `null` quando nenhuma informou. */
  costUsd: number | null;
  /** Pedidos cobrados pelo plano (premium requests do Copilot). */
  requests: number | null;
  calls: number;
  failures: number;
  turns: number;
  durationMs: number;
  /** Tokens (entrada + saída) que não foram informados pela ferramenta. */
  estimatedTokens: number;
}

export interface UsageBreakdown { key: string; label: string | null; parent: string | null; totals: UsageTotals }
export interface UsageDay { day: string; source: string; inputTokens: number; outputTokens: number }
export interface QuotaView { agent: string; window: string; usedPercent: number | null; resetsAt: string | null; plan: string | null; capturedAt: string }

export interface UsageReport {
  totals: UsageTotals;
  daily: UsageDay[];
  bySource: UsageBreakdown[];
  byModel: UsageBreakdown[];
  byProject: UsageBreakdown[];
  byChat: UsageBreakdown[];
  quotas: QuotaView[];
  jev: { work: Record<string, number>; saved: Record<string, number> };
}

export interface TurnUsage { turnId: string; inputTokens: number; outputTokens: number; costUsd: number | null; durationMs: number; estimated: boolean }

/** Como os pedidos passaram pela portaria numa janela de dias. */
export interface GateHistory { checks: number; passed: number; asked: number; blocked: number }
export interface LevelSuggestion { level: Expertise; history: GateHistory; days: number }

/** Uma nota ou receita da memória do projeto. */
export type NoteKind = "note" | "recipe";
export interface ProjectNote {
  id: string;
  projectId: string;
  kind: NoteKind;
  title: string;
  body: string;
  /** O pedido que leva a receita junto. */
  trigger: string;
  /** O critério da portaria que a nota responde, ou vazio. */
  covers: string;
  /** `manual`, `gate` (aprendida da portaria) ou `repeat` (de um pedido repetido). */
  source: string;
  createdAt: string;
  updatedAt: string;
}
export interface NoteDraft { id: string | null; projectId: string; kind: NoteKind; title: string; body: string; trigger: string }
export interface RepeatedRequest { prompt: string; count: number }
export interface ProjectMemory { notes: ProjectNote[]; repeated: RepeatedRequest[]; notesLimit: number; recipeLimit: number }
/** Um chat achado pela busca; o trecho marca o que casou entre \u0002 e \u0003. */
export interface SearchHit { chatId: string; title: string; role: string; snippet: string; at: string }
