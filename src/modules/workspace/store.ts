import { create } from "zustand";
import { bus, commands, onCore, WORK_MODES, type WorkMode, type WorkspaceData } from "@/modules/core";
import { reportError } from "@/modules/feedback";
import { navigate } from "@/modules/navigation";
import { chatsOf, findProject } from "./selectors";

export type Layout = "grid" | "list";
const LAYOUT_KEY = "jayv.layout";

interface WorkspaceState {
  data: WorkspaceData;
  activeProjectId: string | null;
  activeChatId: string | null;
  layout: Layout;
}

export const useWorkspace = create<WorkspaceState>(() => ({
  data: { projects: [], chats: [] },
  activeProjectId: null,
  activeChatId: null,
  layout: localStorage.getItem(LAYOUT_KEY) === "list" ? "list" : "grid",
}));

/** Conta aos outros módulos qual projeto está aberto e quais chats são dele. */
function announceScope() {
  const { data, activeProjectId } = useWorkspace.getState();
  bus.emit("scope:changed", { project: findProject(data, activeProjectId), chats: chatsOf(data, activeProjectId) });
}

/** Relê o banco. O chat preferido, quando existe, passa a ser o aberto; senão
 * o aberto só continua se ainda pertencer ao projeto. */
export async function loadWorkspace(preferredChatId?: string | null) {
  const data = await commands.getWorkspace();
  const state = useWorkspace.getState();
  let activeProjectId = data.projects.some((project) => project.id === state.activeProjectId) ? state.activeProjectId : null;
  let activeChatId = state.activeChatId;
  const preferred = data.chats.find((chat) => chat.id === preferredChatId);
  if (preferred) {
    activeProjectId = preferred.projectId;
    activeChatId = preferred.id;
  } else if (!chatsOf(data, activeProjectId).some((chat) => chat.id === activeChatId)) {
    activeChatId = chatsOf(data, activeProjectId)[0]?.id ?? null;
  }
  useWorkspace.setState({ data, activeProjectId, activeChatId });
  announceScope();
  bus.emit("workspace:loaded", { chats: data.chats });
}

/** Relê o banco mantendo a conversa aberta. */
export function refreshWorkspace() {
  return loadWorkspace(useWorkspace.getState().activeChatId).catch(reportError);
}

export function setLayout(layout: Layout) {
  localStorage.setItem(LAYOUT_KEY, layout);
  useWorkspace.setState({ layout });
}

/** Fixa o modo de trabalho do chat. A tela muda na hora; se o núcleo recusar,
 * o modo volta ao que era e o erro aparece. */
export async function setWorkMode(chatId: string, mode: WorkMode) {
  const before = useWorkspace.getState().data.chats.find((chat) => chat.id === chatId)?.workMode ?? "auto";
  const put = (workMode: WorkMode) => useWorkspace.setState((state) => ({
    data: { ...state.data, chats: state.data.chats.map((chat) => (chat.id === chatId ? { ...chat, workMode } : chat)) },
  }));
  put(mode);
  try {
    await commands.setWorkMode(chatId, mode);
  } catch (error) {
    put(before);
    reportError(error);
  }
}

/** O próximo modo na ordem do seletor: automático, planejamento, desenvolvimento. */
export function nextWorkMode(mode: WorkMode): WorkMode {
  return WORK_MODES[(WORK_MODES.indexOf(mode) + 1) % WORK_MODES.length];
}

/** Entra no projeto: a grade de chats dele é a primeira coisa que aparece. */
export function openProject(projectId: string) {
  const { data, activeChatId } = useWorkspace.getState();
  const chats = chatsOf(data, projectId);
  const keep = chats.some((chat) => chat.id === activeChatId);
  useWorkspace.setState({ activeProjectId: projectId, activeChatId: keep ? activeChatId : chats[0]?.id ?? null });
  announceScope();
  navigate("chats");
}

export function openChat(chatId: string) {
  const chat = useWorkspace.getState().data.chats.find((item) => item.id === chatId);
  if (!chat) return;
  useWorkspace.setState({ activeProjectId: chat.projectId, activeChatId: chatId });
  announceScope();
  navigate("chat");
}

export function leaveProject() {
  useWorkspace.setState({ activeProjectId: null, activeChatId: null });
  announceScope();
  navigate("projects");
}

export async function createChat(projectId: string) {
  try {
    const chat = await commands.createChat(projectId);
    await loadWorkspace(chat.id);
    navigate("chat");
  } catch (error) {
    reportError(error);
  }
}

/** Cria o projeto já com um chat aberto: é para conversar que ele existe. */
export async function createProject(name: string, rootPath: string | null) {
  const project = await commands.createProject(name, rootPath);
  const chat = await commands.createChat(project.id);
  await loadWorkspace(chat.id);
  navigate("chat");
}

/** Abre, no programa que o sistema usa para o tipo, um arquivo que a resposta
 * citou. O núcleo procura o caminho dentro da pasta do projeto do chat. */
export async function openFile(chatId: string, path: string) {
  try {
    await commands.openFile(chatId, path);
  } catch (error) {
    reportError(error);
  }
}

export async function deleteChat(chatId: string) {
  try {
    await commands.deleteChat(chatId);
    if (useWorkspace.getState().activeChatId === chatId) useWorkspace.setState({ activeChatId: null });
    await loadWorkspace();
  } catch (error) {
    reportError(error);
  }
}

export async function deleteProject(projectId: string) {
  try {
    if (useWorkspace.getState().activeProjectId === projectId) useWorkspace.setState({ activeProjectId: null, activeChatId: null });
    await commands.deleteProject(projectId);
    await loadWorkspace();
    navigate("projects");
  } catch (error) {
    reportError(error);
  }
}

/** O núcleo grava o pedido no banco antes de chamar qualquer modelo e avisa
 * aqui. A partir deste aviso a mensagem existe em disco, e o estado da tela tem
 * de saber disso na hora: sem isto, sair do chat redesenharia a conversa a
 * partir do retrato anterior ao envio — e o pedido sumiria da tela mesmo
 * estando gravado.
 *
 * O núcleo também troca o título do chat depois da primeira resposta: a
 * lateral, o cabeçalho e as referências da portaria passam a mostrar o nome
 * novo sem reler a conversa. */
export function connectWorkspace() {
  const offs = [
    onCore("chat-prompt", () => void refreshWorkspace()),
    onCore("turn-settled", () => void refreshWorkspace()),
    onCore("chat-renamed", ({ chatId, title }) => {
      const { data } = useWorkspace.getState();
      if (!data.chats.some((chat) => chat.id === chatId)) return;
      useWorkspace.setState({ data: { ...data, chats: data.chats.map((chat) => (chat.id === chatId ? { ...chat, title } : chat)) } });
      announceScope();
    }),
  ];
  const offSent = bus.on("prompt:sent", ({ chatId }) => {
    const { activeChatId } = useWorkspace.getState();
    void loadWorkspace(activeChatId === chatId ? chatId : undefined).catch(reportError);
  });
  return () => {
    offSent();
    offs.forEach((off) => void off.then((unlisten) => unlisten()));
  };
}
