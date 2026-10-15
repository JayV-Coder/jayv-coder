// O núcleo do app (o Rust, pelo `invoke` do Tauri), de mentira: cada comando
// que a tela chama devolve os dados de exemplo. O que muda alguma coisa
// (salvar, apagar) só devolve o estado de antes — o estúdio não grava nada.
import {
  APP_VERSION, chats, coreSnapshot, gateFeed, LIVE_FILE, LIVE_FILES, MCP_SERVERS, modsSnapshot, PROJECTS, projectMemory, REPOSITORY_STATES, searchHits, settingsSnapshot,
  SKILL_HITS, SKILLS, systemStatus, usageReport, USER, type StudioData,
} from "./fixtures";

declare global {
  interface Window { __studioData: StudioData; __studioCalls: string[] }
}

const data = () => window.__studioData;
/** O mod que o print quer fora dos instalados (`?uninstalled=<id>`). */
const uninstalled = new URLSearchParams(location.search).get("uninstalled");
/** As configurações sem o agente do mod que está fora (o motor `copilot` ou o
 * agente `mod-lm-studio`): a aba dele some, como no app. */
const settings = () => {
  const snapshot = settingsSnapshot();
  if (!uninstalled) return snapshot;
  const gone = (id: string) => id === uninstalled || id === `mod-${uninstalled}`;
  return { ...snapshot, settings: { ...snapshot.settings, agents: snapshot.settings.agents.filter((agent) => !gone(agent.id)), models: snapshot.settings.models.filter((model) => !gone(model.agent)) } };
};
const say = (key: string) => data().samples[key] ?? key;
let cachedChats: ReturnType<typeof chats> | null = null;
const allChats = () => (cachedChats ??= chats(say));

const NO_GRANTS = { shell: false, git: false, network: false, commands: [] };

const HANDLERS: Record<string, (args: Record<string, unknown>) => unknown> = {
  get_workspace: () => ({ projects: PROJECTS, chats: allChats() }),
  get_chat: ({ chatId }) => allChats().find((chat) => chat.id === chatId) ?? null,
  get_settings: () => settings(),
  save_settings: () => settings(),
  get_core_settings: () => coreSnapshot(),
  save_core_settings: () => coreSnapshot(),
  save_expertise: () => coreSnapshot(),
  save_lean_code: () => coreSnapshot(),
  get_mods: () => modsSnapshot(data().catalog?.mods ?? null, uninstalled),
  save_mods: () => modsSnapshot(data().catalog?.mods ?? null, uninstalled),
  get_mcp_servers: () => MCP_SERVERS,
  save_mcp_servers: () => MCP_SERVERS,
  check_mcp_servers: () => MCP_SERVERS,
  get_skills: () => SKILLS,
  set_skill_enabled: () => SKILLS,
  remove_skill: () => SKILLS,
  get_org_extensions: () => ({ mcp: [], skills: [] }),
  search_skill_hub: () => SKILL_HITS,
  preview_skills: () => [],
  chat_grants: () => NO_GRANTS,
  set_chat_grants: () => null,
  allowed_commands: () => [],
  gate_feed: () => gateFeed(say),
  scoped_gate_feed: () => gateFeed(say),
  turn_evidence: () => ({ entry: { score: 84, demand: 50, verdict: "pass", scopeLevel: 1, doneCriterion: true }, exits: [], review: null }),
  live_files: ({ chatId }) => (chatId === "c-live"
    ? { turnId: "c-live-live", running: true, files: LIVE_FILES, folder: "/home/dev/invoice-service", mode: "git", looks: 6 }
    : { turnId: null, running: false, files: [], folder: null, mode: null, looks: 0 }),
  live_file: ({ path }) => (path === LIVE_FILE.path ? LIVE_FILE : { ...LIVE_FILE, path, before: "", after: "" }),
  editors: () => ["code"],
  repository_states: ({ folder }) => (folder === PROJECTS[0].rootPath ? REPOSITORY_STATES : []),
  folder_repo_keys: () => [],
  system_status: () => systemStatus(),
  usage_report: ({ query }) => usageReport(allChats(), (query as { scope?: { kind: string } } | undefined)?.scope),
  chat_usage: ({ chatId }) => (allChats().find((chat) => chat.id === chatId)?.turns ?? []).map((turn, index) => ({ turnId: turn.id, inputTokens: 18_400 + index * 3100, outputTokens: 2100 + index * 400, costUsd: 0.14 + index * 0.03, durationMs: 41_000, estimated: false })),
  refresh_quotas: () => [],
  project_memory: () => projectMemory(say),
  save_project_note: () => projectMemory(say),
  search_chats: ({ query }) => searchHits(allChats(), String(query ?? "")),
  connection_status: () => ({ link: "online", pending: 0, refusals: { byChat: {}, byProject: {}, unplaced: 0 } }),
  current_environment: () => "personal",
  set_environment: () => "personal",
  set_session: () => ({ userId: USER.id, email: USER.email, expiresAt: Date.now() + 3_600_000 }),
  get_locales: () => data().locales,
  get_translations: ({ locale }) => (locale === data().locale ? data().translations : {}),
  check_agent: ({ command }) => ({ path: `/usr/local/bin/${String(command || "agent")}`, version: "2.4.1", loggedIn: true }),
  check_gateway: () => ({ models: 312, error: null }),
  refresh_models: () => ({ snapshot: settings(), listed: true }),
  app_version: () => APP_VERSION,
};

export async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  (window.__studioCalls ??= []).push(command);
  const handler = HANDLERS[command];
  return (handler ? handler(args) : null) as T;
}

export const convertFileSrc = (path: string) => path;
export const isTauri = () => false;
