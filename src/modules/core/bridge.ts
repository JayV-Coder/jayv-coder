import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AgentId, AgentProbe, Chat, CoreSettings, CoreSnapshot, Expertise, EntryCheck, ExitCheck, GateFeed, LlmSettings, ModelsRefresh, Project,
  QuotaView, SettingsSnapshot, SystemStatus, Turn, TurnUsage, UsageReport, UsageScope, WorkspaceData,
} from "./types";
import type { Text } from "@/modules/i18n";

/** Os comandos do núcleo, com nome e formato. É o único lugar da tela que
 * chama `invoke`: os módulos pedem por aqui, e a assinatura de cada comando
 * fica escrita uma vez só. */
export const commands = {
  getWorkspace: () => invoke<WorkspaceData>("get_workspace"),
  createProject: (name: string, rootPath: string | null) => invoke<Project>("create_project", { name, rootPath }),
  organizationProject: (orgId: string, name: string, folder: string) => invoke<Project>("organization_project", { orgId, name, folder }),
  createChat: (projectId: string, title: string | null = null) => invoke<Chat>("create_chat", { projectId, title }),
  clearChat: (chatId: string) => invoke<void>("clear_chat", { chatId }),
  deleteChat: (chatId: string) => invoke<void>("delete_chat", { chatId }),
  deleteProject: (projectId: string) => invoke<void>("delete_project", { projectId }),
  enqueuePrompt: (input: string, sessionId: string, turnId: string | null = null) =>
    invoke<Turn>("enqueue_prompt", { request: { input, sessionId, turnId } }),
  answerQuestion: (questionTurnId: string, picked: string[], text: string | null) =>
    invoke<Turn>("answer_question", { answer: { questionTurnId, picked, text } }),
  dismissQuestion: (questionTurnId: string) => invoke<void>("dismiss_question", { questionTurnId }),
  gateFeed: (projectId: string | null) => invoke<GateFeed>("gate_feed", { projectId }),
  scopedGateFeed: (projectIds: string[], chatId: string | null) => invoke<GateFeed>("scoped_gate_feed", { projectIds, chatId }),
  openFile: (chatId: string, path: string) => invoke<void>("open_file", { chatId, path }),
  scanRepositories: (folder: string, keys: string[]) => invoke<FoundRepository[]>("scan_repositories", { folder, keys }),
  cloneRepository: (key: string, folder: string) => invoke<Project>("clone_repository", { key, folder }),
  folderRepoKeys: (path: string) => invoke<string[]>("folder_repo_keys", { path }),
  systemStatus: () => invoke<SystemStatus>("system_status"),
  getSettings: () => invoke<SettingsSnapshot>("get_settings"),
  saveSettings: (settings: LlmSettings) => invoke<SettingsSnapshot>("save_settings", { settings }),
  getCoreSettings: () => invoke<CoreSnapshot>("get_core_settings"),
  saveCoreSettings: (settings: CoreSettings) => invoke<CoreSnapshot>("save_core_settings", { settings }),
  saveExpertise: (level: Expertise) => invoke<CoreSnapshot>("save_expertise", { level }),
  setReplyLanguage: (language: { tag: string; name: string } | null) => invoke<void>("set_reply_language", { language }),
  checkAgent: (command: string) => invoke<AgentProbe>("check_agent", { command }),
  refreshModels: (agent: AgentId) => invoke<ModelsRefresh>("refresh_models", { agent }),
  setSession: (token: string) => invoke<SessionView>("set_session", { token }),
  clearSession: () => invoke<void>("clear_session"),
  connectionStatus: () => invoke<ConnectionStatus>("connection_status"),
  getLocales: () => invoke<{ id: string; name: string; rtl: boolean; position: number }[]>("get_locales"),
  getTranslations: (locale: string) => invoke<Record<string, string | Record<string, string>>>("get_translations", { locale }),
  usageReport: (query: UsageQuery) => invoke<UsageReport>("usage_report", { query }),
  chatUsage: (chatId: string) => invoke<TurnUsage[]>("chat_usage", { chatId }),
  refreshQuotas: () => invoke<QuotaStatus[]>("refresh_quotas"),
};

/** Um clone de repositório da organização achado numa pasta. */
export interface FoundRepository { key: string; path: string }

/** O pedido das estatísticas: o escopo, o intervalo em ISO (aberto onde vier
 * vazio) e o fuso de quem lê, para que "hoje" seja o hoje dele. */
export interface UsageQuery { scope: UsageScope; from: string | null; to: string | null; utcOffsetMinutes: number }
/** O resultado de reler o limite de um agente: vazio quando leu. */
export interface QuotaStatus { agent: string; problem: Text | null }

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
  "gate-entry": { check: EntryCheck };
  "gate-exit": { checks: ExitCheck[] };
  "link-changed": { link: Link };
  "translations-updated": null;
  "models-updated": null;
  "usage-recorded": { projectId: string | null; chatId: string | null };
  "quota-changed": { quota: Omit<QuotaView, "capturedAt">; crossed: number | null };
}

export function onCore<K extends keyof CoreEvents>(event: K, handler: (payload: CoreEvents[K]) => void): Promise<UnlistenFn> {
  return listen<CoreEvents[K]>(event, ({ payload }) => handler(payload));
}
