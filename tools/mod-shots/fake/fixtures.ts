// Os dados de exemplo do estúdio: um projeto genérico, as conversas que cada
// print mostra, os agentes e o resto que o núcleo daria à tela. Os textos das
// conversas vêm de `samples.<idioma>.json`, para o print sair no idioma da
// página; o que é código (arquivos, funções) fica igual em todos.
import type {
  Activity, AgentModel, AgentSettings, Chat, CoreSnapshot, EntryCheck, ExitCheck, GateFeed, KnownModel, McpServer, ModManifest, ModsSnapshot,
  Project, ProjectMemory, RepositoryState, SearchHit, SettingsSnapshot, Skill, SkillHit, SystemStatus, TurnView, UsageReport,
} from "@/modules/core";
import { version } from "../../../package.json";

/** A versão do app no canto da tela: a deste repositório. */
export const APP_VERSION = version;
/** A hora parada de todos os prints (o `shoot.mjs` para o relógio nela). */
export const NOW = Date.parse("2026-10-13T14:30:00Z");
const at = (minutesAgo: number) => new Date(NOW - minutesAgo * 60_000).toISOString();

export const USER = {
  id: "00000000-0000-4000-8000-000000000001", aud: "authenticated", role: "authenticated", email: "dev@example.com",
  app_metadata: { provider: "email" }, user_metadata: { full_name: "Alex Doe" }, created_at: "2026-01-10T12:00:00Z", last_sign_in_at: at(60),
};
export const PROFILE = {
  display_name: "Alex Doe", username: "alexdoe", sex: null, gender: null, gender_custom: null, pronouns: null, pronouns_custom: null, birth_date: null,
  country: null, timezone: null, role: "developer", company: null, completed_at: "2026-01-10T12:05:00Z", username_set_at: "2026-01-10T12:05:00Z", avatar_url: null,
};

/** O que o estúdio recebe do `vite.config.ts`: o catálogo do repositório de
 * mods, as traduções do idioma e os textos das conversas. */
export interface StudioData {
  locale: string;
  catalog: { schema: number; mods: ModManifest[] } | null;
  translations: Record<string, unknown>;
  samples: Record<string, string>;
  locales: { id: string; name: string; rtl: boolean; position: number }[];
}

export const PROJECT: Project = { id: "p1", name: "invoice-service", rootPath: "/home/dev/invoice-service", createdAt: "2026-06-02T10:00:00Z", repoKeys: ["github.com/acme/invoice-service"], orgId: null };
const SECOND: Project = { id: "p2", name: "web-dashboard", rootPath: "/home/dev/web-dashboard", createdAt: "2026-07-14T10:00:00Z", repoKeys: ["github.com/acme/web-dashboard"], orgId: null };

const BUILT_IN = ["claude", "codex", "copilot", "cursor", "kilo", "openrouter", "litellm"] as const;

function agents(): AgentSettings[] {
  const mechanisms = { claude: ["webSearch", "webFetch"], codex: ["webSearch"], copilot: ["webFetch"] };
  return [
    { id: "claude", enabled: true, command: "claude", timeout: 900, options: { permissionMode: "acceptEdits", effort: "auto", fallbackModel: "", maxBudgetUsd: null, blockedTools: [], appendSystemPrompt: "", persistSessions: true, safeMode: false, symbolTools: true, mechanisms: mechanisms.claude, approveMcps: true } },
    { id: "codex", enabled: true, command: "codex", timeout: 900, options: { sandbox: "workspace-write", reasoningEffort: "auto", networkAccess: false, skipGitRepoCheck: false, mechanisms: mechanisms.codex, approveMcps: true } },
    { id: "copilot", enabled: true, command: "copilot", timeout: 900, options: { toolAccess: "edits", blockedTools: [], silent: true, mechanisms: mechanisms.copilot, approveMcps: false } },
    { id: "cursor", enabled: true, command: "cursor-agent", timeout: 900, options: { sandbox: "default", force: false, approveMcps: false } },
    { id: "kilo", enabled: true, command: "kilo", timeout: 900, options: { auto: false, approveMcps: false } },
    { id: "openrouter", enabled: true, command: "", timeout: 300, options: { baseUrl: "https://openrouter.ai/api/v1", approveMcps: false, hasKey: true } },
    { id: "litellm", enabled: true, command: "", timeout: 300, options: { baseUrl: "http://localhost:4000/v1", approveMcps: false, hasKey: true } },
    { id: "mod-ollama", enabled: true, command: "", timeout: 300, options: { name: "Ollama", kind: "api", args: [], planArgs: [], edits: false, protocol: "openai", baseUrl: "http://localhost:11434/v1", keyRequired: false, approveMcps: false, hasKey: false } },
    { id: "mod-lm-studio", enabled: true, command: "", timeout: 300, options: { name: "LM Studio", kind: "api", args: [], planArgs: [], edits: false, protocol: "openai", baseUrl: "http://localhost:1234/v1", keyRequired: false, approveMcps: false, hasKey: false } },
  ] as AgentSettings[];
}

const MODELS: [string, string, boolean, "low" | "medium" | "high" | "free", "fast" | "medium" | "slow", number][] = [
  ["claude", "sonnet", true, "medium", "medium", 200_000], ["claude", "opus", true, "high", "slow", 200_000], ["claude", "haiku", true, "low", "fast", 200_000],
  ["codex", "gpt-5.5", true, "medium", "medium", 400_000], ["codex", "gpt-5.5-mini", false, "low", "fast", 400_000],
  ["copilot", "claude-sonnet-4.5", true, "medium", "medium", 200_000], ["copilot", "gpt-5", false, "medium", "medium", 400_000],
  ["cursor", "auto", true, "medium", "medium", 200_000],
  ["kilo", "anthropic/claude-sonnet-4", true, "medium", "medium", 200_000],
  ["openrouter", "anthropic/claude-sonnet-4", true, "medium", "medium", 200_000], ["openrouter", "openai/gpt-5-mini", true, "low", "fast", 400_000], ["openrouter", "google/gemini-2.5-pro", false, "medium", "medium", 1_000_000],
  ["litellm", "gpt-4o-mini", true, "low", "fast", 128_000],
  ["mod-ollama", "llama3.2", true, "free", "medium", 128_000], ["mod-ollama", "qwen2.5-coder", true, "free", "medium", 32_000],
  ["mod-lm-studio", "qwen2.5-coder-7b-instruct", true, "free", "medium", 32_000],
];

function models(): AgentModel[] {
  return MODELS.map(([agent, model, enabled, costClass, speed, contextWindow]) => ({
    agent: agent as AgentModel["agent"], model, enabled, costClass, speed, contextWindow,
    capabilities: agent === "openrouter" || agent === "litellm" || agent.startsWith("mod-") ? ["chat", "reasoning"] : ["chat", "code", "reasoning", "tools"],
  }));
}

function knownModels(): Record<string, KnownModel[]> {
  const out: Record<string, KnownModel[]> = {};
  for (const model of models()) (out[model.agent] ??= []).push({ id: model.model, label: model.model, contextWindow: model.contextWindow, costClass: model.costClass, speed: model.speed, capabilities: model.capabilities });
  return out;
}

export function settingsSnapshot(): SettingsSnapshot {
  return { settings: { agents: agents(), models: models() }, catalog: knownModels() as SettingsSnapshot["catalog"], timeoutRange: [30, 7200], contextRange: [1000, 2_000_000] };
}

export function coreSnapshot(): CoreSnapshot {
  const settings = {
    adaptiveRouting: true, confidenceThreshold: 0.6, preferLocal: false, budgets: { trivial: 4000, simple: 12000, medium: 32000, complex: 80000 }, cacheTtl: 3600,
    exitRules: { read: "allow" as const, write: "ask" as const, shell: "ask" as const }, privacy: { deny: [".env", "*.pem"], localOnly: [], redactSecrets: true },
    agentOrder: [], reviewChanges: true, planFirst: true, parallelTasks: true, keepSessionModel: true, resumeAcrossModes: true, turnCeilingMinutes: 60,
  };
  const level = (id: CoreSnapshot["expertise"], demand: [number, number, number]) => ({ id, scopeDemand: demand, blockMargin: 0.15, confidence: 0.6, buildCeiling: "complex" as const, destructiveThreshold: 0.7 });
  return {
    settings, defaults: settings, expertise: "senior",
    gate: { scopeDemand: [0.3, 0.5, 0.7], blockMargin: 0.15, weights: {}, scopeLevels: ["small change", "feature", "whole system"], noulLine: 0.5 },
    levels: [level("starter", [0.5, 0.7, 0.85]), level("junior", [0.4, 0.6, 0.8]), level("mid", [0.35, 0.55, 0.75]), level("senior", [0.3, 0.5, 0.7]), level("architect", [0.25, 0.45, 0.65])],
    suggestion: null, leanCode: true, confidenceRange: [0.3, 0.95], budgetRange: [1000, 200_000], cacheTtlRange: [0, 86_400], version: APP_VERSION,
  };
}

/** Os mods instalados: todos do catálogo, menos o que o print pede fora
 * (`uninstalled`, o mod que não vem instalado, para o print `enable` mostrá-lo
 * em Disponíveis no catálogo, com o botão Instalar). */
export function modsSnapshot(catalog: ModManifest[] | null, uninstalled: string | null = null): ModsSnapshot {
  const mods = Object.fromEntries((catalog ?? []).filter((mod) => mod.id !== uninstalled).map((mod) => [mod.id, { enabled: true, version: mod.version ?? null, installedAt: "2026-10-12T20:00:00Z" }]));
  return { catalog, fetchedAt: at(4), state: { mods, removed: [] }, active: Object.keys(mods), disabled: [], points: ["llmEngines", "features", "agents", "skills"], appVersion: APP_VERSION };
}

/** O `git status` da pasta do projeto, na aba Repositórios do painel Código:
 * o repositório do projeto num branch à frente do remoto, com mudanças, e um
 * pacote dentro dele, limpo. */
export const REPOSITORY_STATES: RepositoryState[] = [
  {
    path: PROJECT.rootPath, relative: "", key: "github.com/acme/invoice-service", keys: ["github.com/acme/invoice-service"], enclosing: false,
    branch: "feature/invoice-totals", upstream: "origin/feature/invoice-totals", ahead: 2, behind: 0, changed: 3, readable: true, problem: null, error: null,
  },
  {
    path: `${PROJECT.rootPath}/packages/pdf-render`, relative: "packages/pdf-render", key: "github.com/acme/pdf-render", keys: ["github.com/acme/pdf-render"], enclosing: false,
    branch: "main", upstream: "origin/main", ahead: 0, behind: 0, changed: 0, readable: true, problem: null, error: null,
  },
];

export const MCP_SERVERS: McpServer[] = [
  { name: "sequential-thinking", transport: "stdio", command: "npx", args: ["-y", "@modelcontextprotocol/server-sequential-thinking"], env: {}, url: "", headers: {}, enabled: true, agents: [] },
  { name: "fetch", transport: "stdio", command: "uvx", args: ["mcp-server-fetch"], env: {}, url: "", headers: {}, enabled: true, agents: [] },
  { name: "git", transport: "stdio", command: "uvx", args: ["mcp-server-git"], env: {}, url: "", headers: {}, enabled: true, agents: [] },
  { name: "memory", transport: "stdio", command: "npx", args: ["-y", "@modelcontextprotocol/server-memory"], env: {}, url: "", headers: {}, enabled: false, agents: [] },
  { name: "docs", transport: "http", command: "", args: [], env: {}, url: "https://mcp.example.com/mcp", headers: {}, enabled: true, agents: ["claude", "codex"] },
];

export const SKILLS: Skill[] = [
  { name: "release-notes", description: "Write release notes from the merged pull requests, grouped by Added, Changed and Fixed.", path: "/home/dev/.jayv/skills/release-notes", enabled: true, installedAt: "2026-09-20T10:00:00Z" },
  { name: "pdf-forms", description: "Read and fill PDF forms: list the fields, fill them from data and flatten the result.", path: "/home/dev/.jayv/skills/pdf-forms", enabled: true, installedAt: "2026-09-21T10:00:00Z" },
  { name: "db-migrations", description: "Create and review database migrations that are safe to run on a live database.", path: "/home/dev/.jayv/skills/db-migrations", enabled: false, installedAt: "2026-09-25T10:00:00Z" },
];

export const SKILL_HITS: SkillHit[] = [
  { id: "acme/agent-skills/changelog", name: "changelog", source: "acme/agent-skills", installs: 18_420 },
  { id: "tools-lab/skills/release-notes", name: "release-notes", source: "tools-lab/skills", installs: 9_310 },
  { id: "docs-kit/skills/conventional-commits", name: "conventional-commits", source: "docs-kit/skills", installs: 6_085 },
  { id: "acme/agent-skills/semver", name: "semver", source: "acme/agent-skills", installs: 2_740 },
];

let seq = 0;
const beat = (kind: string, detail: Record<string, unknown>, minutesAgo = 0.3): Activity => ({ at: at(minutesAgo), seq: ++seq, kind, detail });
const route = (provider: string, model: string, mode: "build" | "plan" = "build") => ({ provider, model, mode, agent: "developer" });

/** Os passos de um pedido que o Jev deixou passar, até o agente começar. */
function startBeats(provider: string, model: string, intent = "code", complexity = "simple", extra: Activity[] = []): Activity[] {
  return [
    beat("gate", { verdict: "pass", score: 82, demand: 50 }),
    beat("read", { intent, complexity, source: "jev" }),
    ...extra,
    beat("context", { files: 6, tokens: 9400 }),
    beat("route", { provider, model, mode: "build", agent: "developer", reason: "" }),
    beat("running", {}),
  ];
}

type Say = (key: string) => string;

interface ChatSpec { id: string; title: string; done: [string, string][]; route: ReturnType<typeof route>; flying?: { prompt: string; beats: Activity[] }; minutesAgo: number }

function chatOf(spec: ChatSpec): Chat {
  const messages: Chat["messages"] = [];
  const turns: TurnView[] = [];
  spec.done.forEach(([question, answer], index) => {
    const id = `${spec.id}-t${index + 1}`;
    const when = spec.minutesAgo + (spec.done.length - index) * 6;
    messages.push({ role: "user", content: question, createdAt: at(when), turnId: id }, { role: "assistant", content: answer, createdAt: at(when - 2), turnId: id });
    turns.push({ id, code: `T${index + 1}`, status: "answered", entry: "pass", exit: "cleared", partial: null, activity: [], route: spec.route });
  });
  if (spec.flying) {
    const id = `${spec.id}-live`;
    messages.push({ role: "user", content: spec.flying.prompt, createdAt: at(spec.minutesAgo), turnId: id });
    // Os passos na ordem em que aparecem na lista, numerados de novo.
    const activity = spec.flying.beats.map((item, index) => ({ ...item, seq: index + 1 }));
    turns.push({ id, code: `T${spec.done.length + 1}`, status: "flying", entry: "pass", exit: null, partial: null, activity, route: null });
  }
  const last = messages.filter((message) => message.role === "user").at(-1)?.content ?? null;
  return {
    id: spec.id, code: spec.id.toUpperCase(), projectId: PROJECT.id, title: spec.title, messages, turns, question: null,
    createdAt: at(spec.minutesAgo + 60), updatedAt: at(spec.minutesAgo), workMode: "auto", lastPrompt: last, messageCount: messages.length,
  };
}

/** As conversas do projeto. Cada uma é o cenário de um print: um agente que
 * respondeu, um pedido no ar com os passos do Jev, a revisão, o plano… */
export function chats(say: Say): Chat[] {
  const engine = (id: string, provider: string, model: string, minutesAgo: number): ChatSpec => ({
    id: `c-${id}`, title: say(`${id}.title`), done: [[say(`${id}.q`), say(`${id}.a`)]], route: route(provider, model), minutesAgo,
    flying: { prompt: say("next.q"), beats: [...startBeats(provider, model, "test"), beat("agent", { line: "assistant: Read" }), beat("agent", { line: "assistant: Edit" })] },
  });
  const specs: ChatSpec[] = [
    engine("claude", "claude", "sonnet", 2),
    engine("codex", "codex", "gpt-5.5", 30),
    engine("copilot", "copilot", "claude-sonnet-4.5", 55),
    engine("cursor", "cursor", "auto", 80),
    engine("kilo", "kilo", "anthropic/claude-sonnet-4", 110),
    engine("openrouter", "openrouter", "anthropic/claude-sonnet-4", 140),
    engine("litellm", "litellm", "gpt-4o-mini", 170),
    engine("ollama", "mod-ollama", "llama3.2", 200),
    engine("lmstudio", "mod-lm-studio", "qwen2.5-coder-7b-instruct", 230),
    {
      id: "c-review", title: say("review.title"), route: route("claude", "sonnet"), minutesAgo: 260,
      done: [[say("review.q"), `${say("review.a")}⁣${say("review.r")}`]],
    },
    {
      id: "c-plan", title: say("plan.title"), route: route("codex", "gpt-5.5"), minutesAgo: 1, done: [],
      flying: { prompt: say("plan.q"), beats: [...startBeats("codex", "gpt-5.5", "code", "complex"), beat("plan", { provider: "claude", model: "opus" }), beat("agent", { line: "item.started: file_change" })] },
    },
    {
      id: "c-split", title: say("split.title"), route: route("claude", "sonnet"), minutesAgo: 1, done: [],
      flying: {
        prompt: say("split.q"),
        beats: [
          beat("gate", { verdict: "pass", score: 79, demand: 70 }), beat("read", { intent: "code", complexity: "complex", source: "jev" }), beat("context", { files: 11, tokens: 21_800 }),
          beat("split", { tasks: [{ title: say("split.part1"), provider: "claude" }, { title: say("split.part2"), provider: "codex" }, { title: say("split.part3"), provider: "copilot" }] }),
          beat("subtask", { outcome: "applied", title: say("split.part1"), provider: "claude", patch: "3 files" }),
          beat("subtask", { outcome: "applied", title: say("split.part3"), provider: "copilot", patch: "2 files" }),
        ],
      },
    },
    { id: "c-recall", title: say("recall.title"), route: route("claude", "haiku"), minutesAgo: 300, done: [[say("recall.q"), say("recall.a")]] },
    {
      id: "c-skill", title: say("skill.title"), route: route("claude", "sonnet"), minutesAgo: 1, done: [],
      flying: { prompt: say("skill.q"), beats: [...startBeats("claude", "sonnet", "general", "simple", [beat("skill", { name: "release-notes" })]), beat("agent", { line: "assistant: Bash" })] },
    },
    {
      id: "c-rea", title: say("rea.title"), route: route("claude", "sonnet"), minutesAgo: 1, done: [[say("rea.q"), say("rea.a")]],
      flying: {
        prompt: say("next.q"),
        beats: [...startBeats("claude", "sonnet", "analysis", "medium", [beat("skill", { name: "reverse-engineering" })]), beat("agent", { line: "assistant: mcp__rea__inspect_artifact" }), beat("agent", { line: "assistant: mcp__rea__trace_feature" }), beat("agent", { line: "assistant: mcp__rea__analyze_function" })],
      },
    },
    {
      id: "c-live", title: say("live.title"), route: route("claude", "sonnet"), minutesAgo: 1, done: [],
      flying: { prompt: say("live.q"), beats: [...startBeats("claude", "sonnet", "refactor"), beat("agent", { line: "assistant: Grep" }), beat("agent", { line: "assistant: Edit" })] },
    },
    { id: "c-find", title: say("find.title"), route: route("claude", "haiku"), minutesAgo: 400, done: [[say("find.q"), say("find.a")], [say("find.q2"), say("find.a2")]] },
  ];
  return specs.map(chatOf);
}

export const PROJECTS = [PROJECT, SECOND];

/** As notas da memória do projeto. */
export function projectMemory(say: Say): ProjectMemory {
  const note = (id: string, kind: "note" | "recipe", key: string, trigger = ""): ProjectMemory["notes"][number] => ({
    id, projectId: PROJECT.id, kind, title: say(`${key}.title`), body: say(`${key}.body`), trigger, covers: "", source: "manual", createdAt: at(3000), updatedAt: at(2000),
  });
  return {
    notes: [note("n1", "note", "project.notes"), note("n2", "note", "project.notes2"), note("n3", "recipe", "project.recipe", say("project.recipe.trigger"))],
    repeated: [], notesLimit: 2200, recipeLimit: 10,
  };
}

/** A busca nos chats do projeto: o trecho marca o que casou entre \u0002 e \u0003. */
export function searchHits(chatsList: Chat[], query: string): SearchHit[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return [];
  const hits: SearchHit[] = [];
  for (const chat of chatsList) {
    for (const message of chat.messages) {
      const text = message.content.replace(/[*`#⁣]/g, "").replace(/\s+/g, " ");
      const found = text.toLocaleLowerCase().indexOf(needle);
      if (found < 0) continue;
      const from = Math.max(0, found - 40);
      const snippet = `${from > 0 ? "…" : ""}${text.slice(from, found)}\u0002${text.slice(found, found + needle.length)}\u0003${text.slice(found + needle.length, found + needle.length + 80)}…`;
      hits.push({ chatId: chat.id, title: chat.title, role: message.role, snippet, at: message.createdAt });
      break;
    }
    if (hits.length >= 6) break;
  }
  return hits;
}

/** A portaria do projeto: o que passou, o que ela perguntou e o que segurou. */
export function gateFeed(say: Say): GateFeed {
  const criteria = [
    { id: "scope", label: "scope", percent: 78, band: [60, 100] as [number, number], reading: "", inverted: false },
    { id: "clarity", label: "clarity", percent: 84, band: [55, 100] as [number, number], reading: "", inverted: false },
    { id: "risk", label: "risk", percent: 22, band: [0, 45] as [number, number], reading: "", inverted: true },
  ];
  const entry = (id: string, chatId: string, key: string, verdict: EntryCheck["verdict"], score: number, minutesAgo: number): EntryCheck => ({
    id, at: at(minutesAgo), chatId, turn: "T1", prompt: say(key), score, demand: 50, verdict, scope: "feature", criteria, source: "jev", note: "",
  });
  const entries = [
    entry("e1", "c-claude", "claude.q", "pass", 86, 8), entry("e2", "c-codex", "codex.q", "pass", 81, 36), entry("e3", "c-plan", "plan.q", "ask", 46, 60),
    entry("e4", "c-review", "review.q", "pass", 77, 266), entry("e5", "c-live", "live.q", "pass", 90, 3),
  ];
  const exit = (id: string, chatId: string, kind: string, target: string, verdict: ExitCheck["verdict"], minutesAgo: number, rule: string | null = null): ExitCheck => ({
    id, at: at(minutesAgo), chatId, turnId: `${chatId}-t1`, turn: "T1", kind, target, rule, verdict,
  });
  const exits = [
    exit("x1", "c-claude", "write", "src/dates.rs", "cleared", 6), exit("x2", "c-claude", "write", "tests/dates.rs", "cleared", 6),
    exit("x3", "c-codex", "shell", "cargo test api", "cleared", 33), exit("x4", "c-review", "write", ".env", "held", 262, ".env"),
  ];
  return { entries, exits, tally: { passed: 42, asked: 5, blocked: 1, held: 2 } };
}

/** As estatísticas do escopo pedido: o projeto e o chat levam uma fração do total. */
export function usageReport(chatsList: Chat[], scope: { kind: string } = { kind: "global" }): UsageReport {
  const full = globalUsage(chatsList);
  const share = scope.kind === "chat" ? 0.04 : scope.kind === "project" || scope.kind === "projects" ? 0.8 : 1;
  if (share === 1) return full;
  const scale = <T extends Record<string, unknown>>(totals: T): T => Object.fromEntries(Object.entries(totals).map(([key, value]) => [key, typeof value === "number" ? Math.round(value * share * 100) / 100 : value])) as T;
  return { ...full, totals: scale(full.totals), daily: full.daily.map((day) => ({ ...day, inputTokens: Math.round(day.inputTokens * share), outputTokens: Math.round(day.outputTokens * share) })) };
}

function globalUsage(chatsList: Chat[]): UsageReport {
  const totals = (input: number, output: number, cost: number, calls: number, turns: number) => ({
    inputTokens: input, outputTokens: output, cacheReadTokens: Math.round(input * 0.6), cacheWriteTokens: Math.round(input * 0.1), costUsd: cost, requests: null, calls, failures: 0, turns, durationMs: turns * 41_000, estimatedTokens: 0,
  });
  const sources: [string, number, number, number][] = [["claude", 1_840_000, 212_000, 18.4], ["codex", 960_000, 118_000, 7.9], ["copilot", 410_000, 52_000, 0], ["openrouter", 220_000, 31_000, 1.2], ["mod-ollama", 180_000, 26_000, 0]];
  const daily = [] as UsageReport["daily"];
  for (let day = 13; day >= 0; day--) {
    const date = new Date(NOW - day * 86_400_000).toISOString().slice(0, 10);
    const wave = 1 + Math.sin(day * 1.3) * 0.45;
    for (const [source, input, output] of sources) daily.push({ day: date, source, inputTokens: Math.round((input / 14) * wave), outputTokens: Math.round((output / 14) * wave) });
  }
  return {
    totals: totals(3_610_000, 439_000, 27.5, 412, 96), daily,
    bySource: sources.map(([key, input, output, cost]) => ({ key, label: null, parent: null, totals: totals(input, output, cost, Math.round(input / 9000), Math.round(input / 38_000)) })),
    byModel: [["claude/sonnet", 1_520_000, 170_000, 13.1], ["claude/opus", 320_000, 42_000, 5.3], ["codex/gpt-5.5", 960_000, 118_000, 7.9], ["copilot/claude-sonnet-4.5", 410_000, 52_000, 0]].map(([key, input, output, cost]) => ({ key: String(key), label: null, parent: String(key).split("/")[0], totals: totals(Number(input), Number(output), Number(cost), 40, 12) })),
    byProject: [{ key: PROJECT.id, label: PROJECT.name, parent: null, totals: totals(2_900_000, 351_000, 22.1, 330, 78) }, { key: SECOND.id, label: SECOND.name, parent: null, totals: totals(710_000, 88_000, 5.4, 82, 18) }],
    byChat: chatsList.slice(0, 6).map((chat, index) => ({ key: chat.id, label: chat.title, parent: PROJECT.id, totals: totals(600_000 - index * 70_000, 70_000 - index * 8000, 4.2 - index * 0.5, 30, 6) })),
    quotas: [
      { agent: "claude", window: "session", usedPercent: 38, resetsAt: new Date(NOW + 2.5 * 3_600_000).toISOString(), plan: "Max", capturedAt: at(5) },
      { agent: "claude", window: "week", usedPercent: 61, resetsAt: new Date(NOW + 3 * 86_400_000).toISOString(), plan: "Max", capturedAt: at(5) },
      { agent: "codex", window: "session", usedPercent: 12, resetsAt: new Date(NOW + 4 * 3_600_000).toISOString(), plan: "Plus", capturedAt: at(5) },
    ],
    jev: {
      work: { "entry:pass": 91, "entry:ask": 5, "entry:block": 1, "exit:cleared": 86, "exit:held": 2, cache_hit: 14, cache_miss: 40, secret_redacted: 3, file_withheld: 1, session_resumed: 38, note_learned: 4, answer_recalled: 6 },
      saved: { blocked: 12_400 },
    },
  };
}

export function systemStatus(): SystemStatus {
  return {
    version: APP_VERSION, config_path: "/home/dev/.jayv/config.yaml", database_path: "/home/dev/.jayv/jayv.db", database_name: "jayv.db",
    tables: [{ name: "chats", rows: 18 }, { name: "messages", rows: 412 }, { name: "turns", rows: 206 }], providers: 9, models: 14, indexed_files: 318, cache_entries: 42, session_messages: 96, performance_records: 188,
  };
}

/** Os arquivos que o pedido no ar está mudando, para o painel Ao vivo. */
export const LIVE_FILES = [
  { path: "src/api/invoices.rs", kind: "modified" as const, at: NOW - 20_000 },
  { path: "src/models/invoice.rs", kind: "modified" as const, at: NOW - 45_000 },
  { path: "migrations/20261013_rename_customer_id.sql", kind: "created" as const, at: NOW - 70_000 },
  { path: "tests/api.rs", kind: "modified" as const, at: NOW - 95_000 },
];
export const LIVE_FILE = {
  path: "src/api/invoices.rs",
  before: "pub struct InvoiceQuery {\n    pub customer_ref: Option<String>,\n    pub page: Option<u32>,\n}\n\nfn filter(query: &InvoiceQuery) -> Filter {\n    Filter::new().customer(query.customer_ref.as_deref())\n}\n",
  after: "pub struct InvoiceQuery {\n    pub customer_id: Option<String>,\n    pub page: Option<u32>,\n}\n\nfn filter(query: &InvoiceQuery) -> Filter {\n    Filter::new().customer(query.customer_id.as_deref())\n}\n",
  beforeKnown: true, hidden: false, binary: false, tooLarge: false,
};

export const BUILT_IN_AGENTS = BUILT_IN;
