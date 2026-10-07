import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AgentId, AgentProbe, GatewayCheck, Chat, CoreSettings, Grants, McpDraft, McpServer, OrgExtensions, Skill, CoreSnapshot, Expertise, EntryCheck, ExitCheck, GateFeed, LlmSettings, ModelsRefresh, NoteDraft, Project,
  ProjectMemory, QuotaView, SearchHit, SettingsSnapshot, SystemStatus, Turn, TurnEvidence, TurnUsage, UsageReport, UsageScope, WorkMode, WorkspaceData,
} from "./types";
import type { Text } from "@/modules/i18n";

/** Os comandos do núcleo, com nome e formato. É o único lugar da tela que
 * chama `invoke`: os módulos pedem por aqui, e a assinatura de cada comando
 * fica escrita uma vez só. */
export const commands = {
  getWorkspace: () => invoke<WorkspaceData>("get_workspace"),
  getChat: (chatId: string) => invoke<Chat | null>("get_chat", { chatId }),
  createProject: (name: string, rootPath: string | null) => invoke<Project>("create_project", { name, rootPath }),
  organizationProject: (orgId: string, name: string, folder: string) => invoke<Project>("organization_project", { orgId, name, folder }),
  createChat: (projectId: string, title: string | null = null) => invoke<Chat>("create_chat", { projectId, title }),
  clearChat: (chatId: string) => invoke<void>("clear_chat", { chatId }),
  setWorkMode: (chatId: string, mode: WorkMode) => invoke<void>("set_work_mode", { chatId, mode }),
  deleteChat: (chatId: string) => invoke<void>("delete_chat", { chatId }),
  deleteProject: (projectId: string) => invoke<void>("delete_project", { projectId }),
  enqueuePrompt: (input: string, sessionId: string, turnId: string | null = null, grants: Grants | null = null) =>
    invoke<Turn>("enqueue_prompt", { request: { input, sessionId, turnId, grants } }),
  answerQuestion: (questionTurnId: string, picked: string[], text: string | null) =>
    invoke<Turn | null>("answer_question", { answer: { questionTurnId, picked, text } }),
  getMcpServers: () => invoke<McpServer[]>("get_mcp_servers"),
  saveMcpServers: (servers: McpServer[]) => invoke<McpServer[]>("save_mcp_servers", { servers }),
  draftMcp: (text: string) => invoke<McpDraft>("draft_mcp", { text }),
  getSkills: () => invoke<Skill[]>("get_skills"),
  getOrgExtensions: () => invoke<OrgExtensions>("get_org_extensions"),
  installSkillFolder: (path: string) => invoke<Skill[]>("install_skill_folder", { path }),
  installSkillText: (text: string) => invoke<Skill[]>("install_skill_text", { text }),
  setSkillEnabled: (name: string, enabled: boolean) => invoke<Skill[]>("set_skill_enabled", { name, enabled }),
  removeSkill: (name: string) => invoke<Skill[]>("remove_skill", { name }),
  chatGrants: (chatId: string) => invoke<Grants>("chat_grants", { chatId }),
  setChatGrants: (chatId: string, grants: Grants) => invoke<void>("set_chat_grants", { chatId, grants }),
  allowedCommands: (chatId: string) => invoke<string[]>("allowed_commands", { chatId }),
  forgetAllowedCommand: (chatId: string, command: string) => invoke<string[]>("forget_allowed_command", { chatId, command }),
  dismissQuestion: (questionTurnId: string) => invoke<void>("dismiss_question", { questionTurnId }),
  cancelTurn: (turnId: string) => invoke<void>("cancel_turn", { turnId }),
  gateFeed: (projectId: string | null) => invoke<GateFeed>("gate_feed", { projectId }),
  scopedGateFeed: (projectIds: string[], chatId: string | null) => invoke<GateFeed>("scoped_gate_feed", { projectIds, chatId }),
  turnEvidence: (turnId: string) => invoke<TurnEvidence>("turn_evidence", { turnId }),
  openFile: (chatId: string, path: string) => invoke<void>("open_file", { chatId, path }),
  scanRepositories: (folder: string) => invoke<FolderScan>("scan_repositories", { folder }),
  cloneRepository: (key: string, folder: string) => invoke<Project>("clone_repository", { key, folder }),
  folderRepoKeys: (path: string) => invoke<string[]>("folder_repo_keys", { path }),
  repositoryStates: (folder: string) => invoke<RepositoryState[]>("repository_states", { folder }),
  liveFiles: (chatId: string) => invoke<LiveList>("live_files", { chatId }),
  liveFile: (chatId: string, path: string) => invoke<LiveFile | null>("live_file", { chatId, path }),
  editors: () => invoke<string[]>("editors"),
  setTrayLabels: (labels: TrayLabels) => invoke<void>("set_tray_labels", { labels }),
  openInEditor: (chatId: string, path: string, line: number, editor: string) => invoke<void>("open_in_editor", { chatId, path, line, editor }),
  systemStatus: () => invoke<SystemStatus>("system_status"),
  getSettings: () => invoke<SettingsSnapshot>("get_settings"),
  saveSettings: (settings: LlmSettings) => invoke<SettingsSnapshot>("save_settings", { settings }),
  getCoreSettings: () => invoke<CoreSnapshot>("get_core_settings"),
  saveCoreSettings: (settings: CoreSettings) => invoke<CoreSnapshot>("save_core_settings", { settings }),
  saveExpertise: (level: Expertise) => invoke<CoreSnapshot>("save_expertise", { level }),
  saveLeanCode: (enabled: boolean) => invoke<CoreSnapshot>("save_lean_code", { enabled }),
  setReplyLanguage: (language: { tag: string; name: string } | null) => invoke<void>("set_reply_language", { language }),
  checkAgent: (command: string, agent: AgentId | null = null) => invoke<AgentProbe>("check_agent", { command, agent }),
  checkGateway: (agent: AgentId) => invoke<GatewayCheck>("check_gateway", { agent }),
  refreshModels: (agent: AgentId) => invoke<ModelsRefresh>("refresh_models", { agent }),
  setSession: (token: string) => invoke<SessionView>("set_session", { token }),
  clearSession: () => invoke<void>("clear_session"),
  connectionStatus: () => invoke<ConnectionStatus>("connection_status"),
  getLocales: () => invoke<{ id: string; name: string; rtl: boolean; position: number }[]>("get_locales"),
  getTranslations: (locale: string) => invoke<Record<string, string | Record<string, string>>>("get_translations", { locale }),
  usageReport: (query: UsageQuery) => invoke<UsageReport>("usage_report", { query }),
  chatUsage: (chatId: string) => invoke<TurnUsage[]>("chat_usage", { chatId }),
  refreshQuotas: () => invoke<QuotaStatus[]>("refresh_quotas"),
  projectMemory: (projectId: string) => invoke<ProjectMemory>("project_memory", { projectId }),
  saveProjectNote: (draft: NoteDraft) => invoke<ProjectMemory>("save_project_note", { draft }),
  deleteProjectNote: (projectId: string, id: string) => invoke<ProjectMemory>("delete_project_note", { projectId, id }),
  searchChats: (projectId: string, query: string) => invoke<SearchHit[]>("search_chats", { projectId, query }),
};

/** Um clone achado numa pasta, com as chaves dos remotes dele (vazia num
 * repositório sem remote do GitHub, do GitLab ou do Bitbucket). */
export interface LocalClone { path: string; keys: string[] }

/** Os clones de uma pasta, em qualquer profundidade. `truncated`: a busca
 * parou num dos limites (pasta funda ou grande demais) antes de olhar tudo. */
export interface FolderScan { clones: LocalClone[]; truncated: boolean }

/** Um repositório dentro da pasta de um chat, com o que o `git status` diz.
 * `relative` vazio é a própria pasta; `readable` falso, o git não respondeu. */
export interface RepositoryState {
  path: string; relative: string; key: string | null; branch: string | null; upstream: string | null;
  ahead: number; behind: number; changed: number; readable: boolean;
}

/** Um arquivo que mudou durante o último pedido do chat; `at` em ms. */
export interface LiveChange { path: string; kind: "created" | "modified" | "removed"; at: number }
/** O aviso de um arquivo: `discarded` é o que nasceu e sumiu durante o pedido
 * (um temporário) e sai da lista. */
export interface LiveNotice { path: string; kind: LiveChange["kind"] | "discarded"; at: number }
/** `mode`: `git` (olha o `git status`) ou `folder` (pasta sem git); nulo enquanto
 * nenhum pedido deste chat olhou a pasta. `looks` conta as olhadas já feitas. */
export interface LiveList { turnId: string | null; running: boolean; files: LiveChange[]; folder: string | null; mode: "git" | "folder" | null; looks: number }
/** O antes (como estava quando o pedido começou) e o agora de um arquivo.
 * `beforeKnown` falso: pasta sem git, sem como saber o antes. */
export interface LiveFile {
  path: string; before: string | null; after: string | null; beforeKnown: boolean; hidden: boolean; binary: boolean; tooLarge: boolean;
}

/** O pedido das estatísticas: o escopo, o intervalo em ISO (aberto onde vier
 * vazio) e o fuso de quem lê, para que "hoje" seja o hoje dele. */
export interface UsageQuery { scope: UsageScope; from: string | null; to: string | null; utcOffsetMinutes: number }
/** O resultado de reler o limite de um agente: vazio quando leu. */
export interface QuotaStatus { agent: string; problem: Text | null }

/** O texto do menu da bandeja, no idioma da tela. */
export interface TrayLabels {
  open: string; newChat: string; projects: string; organizations: string; stats: string; system: string; settings: string; update: string; quit: string; tooltip: string;
}

/** A sessão que o núcleo aceitou. */
export interface SessionView { userId: string; email: string | null; expiresAt: number }

/** A conexão com o Supabase, como o motor de sincronização a vê. */
export type Link = "signedOut" | "offline" | "online" | "expired";
/** O que o servidor recusou, por dono: cada chat, cada projeto (com os chats
 * dele somados) e o que não é de projeto nenhum. */
export interface Refusals { byChat: Record<string, number>; byProject: Record<string, number>; unplaced: number }
export interface ConnectionStatus { link: Link; pending: number; refusals: Refusals }

/** Os avisos que o núcleo manda (ver `src-tauri/src/desktop/events.rs`). */
export interface CoreEvents {
  "chat-prompt": { chatId: string };
  "turn-settled": { chatId: string; turnId: string };
  "chat-renamed": { chatId: string; title: string };
  "turn-beat": { chatId: string; turnId: string; seq: number; kind: string; detail: Record<string, unknown> };
  "turn-chunk": { chatId: string; turnId: string; text: string };
  "live-file": { chatId: string; turnId: string; file: LiveNotice | null };
  "tray-action": { action: string };
  "update-found": { version: string };
  "gate-entry": { check: EntryCheck };
  "gate-exit": { checks: ExitCheck[] };
  "link-changed": { link: Link };
  "translations-updated": null;
  "models-updated": null;
  /** `now` é falso quando um pedido no ar deixou a troca para o próximo. */
  "settings-applied": { now: boolean };
  "usage-recorded": { projectId: string | null; chatId: string | null };
  "quota-changed": { quota: Omit<QuotaView, "capturedAt">; crossed: number | null };
}

export function onCore<K extends keyof CoreEvents>(event: K, handler: (payload: CoreEvents[K]) => void): Promise<UnlistenFn> {
  return listen<CoreEvents[K]>(event, ({ payload }) => handler(payload));
}
