import type { Chat, Project, TurnView, WorkspaceData } from "@/modules/core";
import { t } from "@/modules/i18n";

export const RECENT_CHATS = 3;

/** O título do chat. Chat ainda sem nome é gravado sem título, e a tela diz
 * "novo chat" no idioma de quem lê; `Novo chat` é a grafia dos registros
 * gravados antes. */
export function chatTitle(chat: { title: string }): string {
  return chat.title.trim() && chat.title !== "Novo chat" ? chat.title : t("common.newChat");
}

export function chatsOf(data: WorkspaceData, projectId: string | null): Chat[] {
  return data.chats
    .filter((chat) => chat.projectId === projectId)
    .sort((a, b) => new Date(b.updatedAt).valueOf() - new Date(a.updatedAt).valueOf());
}

export function findProject(data: WorkspaceData, projectId: string | null): Project | null {
  return data.projects.find((project) => project.id === projectId) ?? null;
}

export function findChat(data: WorkspaceData, chatId: string | null): Chat | null {
  return data.chats.find((chat) => chat.id === chatId) ?? null;
}

/** Os pedidos deste chat que ainda não foram atendidos, do mais antigo para o
 * mais novo. Sai do retrato do banco, e não de um registro da tela: é por isso
 * que a espera continua desenhada depois de sair do chat, de recarregar a
 * janela ou de fechar e reabrir o aplicativo. */
export function openTurns(chat: Chat | null | undefined): TurnView[] {
  return (chat?.turns ?? []).filter((turn) => turn.status === "queued" || turn.status === "flying");
}

/** Os três chats mais recentes do projeto. O chat aberto entra na lista mesmo
 * quando é antigo: é nela que se vê qual conversa está na tela, e a grade de
 * chats continua mostrando o projeto inteiro. */
export function recentChats(chats: Chat[], openChatId: string | null): Chat[] {
  const recent = chats.slice(0, RECENT_CHATS);
  const open = chats.find((chat) => chat.id === openChatId);
  if (!open || recent.includes(open)) return recent;
  return [...recent.slice(0, RECENT_CHATS - 1), open];
}

/** O nome que a pasta escolhida sugere: o último trecho do caminho, com barra
 * do Windows ou do Unix. */
export function folderName(path: string) {
  return path.split(/[\\/]+/).filter(Boolean).pop() ?? "";
}

/** O projeto que já usa a pasta, comparada sem barra no fim. É só o aviso
 * antecipado: o núcleo confere de novo, com o caminho real, ao criar. */
export function folderOwner(projects: Project[], path: string) {
  const key = (value: string) => value.trim().replace(/[\\/]+$/, "");
  const wanted = key(path);
  return wanted ? projects.find((project) => key(project.rootPath) === wanted) ?? null : null;
}
