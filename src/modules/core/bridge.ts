import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AgentProbe, Chat, EntryCheck, ExitCheck, GateFeed, LlmSettings, Project,
  SettingsSnapshot, SystemStatus, Turn, WorkspaceData,
} from "./types";

/** Os comandos do núcleo, com nome e formato. É o único lugar da tela que
 * chama `invoke`: os módulos pedem por aqui, e a assinatura de cada comando
 * fica escrita uma vez só. */
export const commands = {
  getWorkspace: () => invoke<WorkspaceData>("get_workspace"),
  createProject: (name: string, rootPath: string | null) => invoke<Project>("create_project", { name, rootPath }),
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
  openFile: (chatId: string, path: string) => invoke<void>("open_file", { chatId, path }),
  systemStatus: () => invoke<SystemStatus>("system_status"),
  getSettings: () => invoke<SettingsSnapshot>("get_settings"),
  saveSettings: (settings: LlmSettings) => invoke<SettingsSnapshot>("save_settings", { settings }),
  checkAgent: (command: string) => invoke<AgentProbe>("check_agent", { command }),
  setSession: (token: string) => invoke<SessionView>("set_session", { token }),
  clearSession: () => invoke<void>("clear_session"),
  connectionStatus: () => invoke<ConnectionStatus>("connection_status"),
  getLocales: () => invoke<{ id: string; name: string; rtl: boolean; position: number }[]>("get_locales"),
  getTranslations: (locale: string) => invoke<Record<string, string | Record<string, string>>>("get_translations", { locale }),
};

/** A sessão que o núcleo aceitou. */
export interface SessionView { userId: string; email: string | null; expiresAt: number }

/** A conexão com o Supabase, como o motor de sincronização a vê. */
export type Link = "signedOut" | "offline" | "online" | "expired";
export interface ConnectionStatus { link: Link; pending: number; failed: number }

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
}

export function onCore<K extends keyof CoreEvents>(event: K, handler: (payload: CoreEvents[K]) => void): Promise<UnlistenFn> {
  return listen<CoreEvents[K]>(event, ({ payload }) => handler(payload));
}
