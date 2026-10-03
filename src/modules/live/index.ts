import { create } from "zustand";
import { commands, onCore, type LiveChange, type LiveFile } from "@/modules/core";
import { reportError } from "@/modules/feedback";
import { useWorkspace } from "@/modules/workspace";
import { firstChangedLine, lineDiff } from "./diff";
import { withChange } from "./changes";

export * from "./diff";
export * from "./changes";

/** Os arquivos que o último pedido de um chat mudou, do mais recente para o
 * mais antigo. */
export interface LiveChat { turnId: string | null; files: LiveChange[] }

interface LiveState {
  chats: Record<string, LiveChat>;
  /** O painel aberto ou fechado em cada chat. Sem escolha ainda, ele abre
   * sozinho quando o primeiro arquivo muda. */
  panel: Record<string, boolean>;
  /** O arquivo mostrado em cada chat. */
  selected: Record<string, string | null>;
  /** Seguir o agente: o arquivo que acabou de mudar passa a ser o mostrado. */
  follow: boolean;
  /** O editor que acompanha o agente (`code`, `cursor`…), ou nenhum. */
  editor: string | null;
  /** Os editores com linha de comando deste computador; nulo antes de olhar. */
  editors: string[] | null;
}

const FOLLOW_KEY = "jayv.live.follow";
const EDITOR_KEY = "jayv.live.editor";

function read(key: string): string | null {
  try { return localStorage.getItem(key); } catch { return null; }
}
function write(key: string, value: string | null) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    // Sem armazenamento, a escolha vale até fechar o app.
  }
}

export const useLive = create<LiveState>(() => ({
  chats: {}, panel: {}, selected: {}, follow: read(FOLLOW_KEY) !== "0", editor: read(EDITOR_KEY), editors: null,
}));

/** Lê a lista do chat no núcleo: ao abrir o chat, ou ao voltar a ele. */
export async function loadLive(chatId: string) {
  const list = await commands.liveFiles(chatId);
  useLive.setState((state) => ({ chats: { ...state.chats, [chatId]: { turnId: list.turnId, files: list.files } } }));
}

export function setLivePanel(chatId: string, open: boolean) {
  useLive.setState((state) => ({ panel: { ...state.panel, [chatId]: open } }));
}

export function selectLiveFile(chatId: string, path: string | null) {
  // Escolher outro arquivo à mão é parar de seguir o agente.
  useLive.setState((state) => ({ selected: { ...state.selected, [chatId]: path }, follow: false }));
  write(FOLLOW_KEY, "0");
}

export function setLiveFollow(follow: boolean) {
  useLive.setState({ follow });
  write(FOLLOW_KEY, follow ? "1" : "0");
}

export function setLiveEditor(editor: string | null) {
  useLive.setState({ editor });
  write(EDITOR_KEY, editor);
}

export async function loadEditors() {
  if (useLive.getState().editors) return;
  const editors = await commands.editors().catch(() => [] as string[]);
  useLive.setState((state) => ({ editors, editor: state.editor && editors.includes(state.editor) ? state.editor : null }));
}

export function liveFile(chatId: string, path: string): Promise<LiveFile | null> {
  return commands.liveFile(chatId, path);
}

/** Abre o arquivo no editor, na primeira linha que mudou. */
export async function openLiveInEditor(chatId: string, path: string, editor: string) {
  const view = await liveFile(chatId, path);
  const line = view?.after != null ? firstChangedLine(lineDiff(view.before ?? "", view.after)) : 1;
  await commands.openInEditor(chatId, path, line, editor);
}

/** O editor que segue o agente não pula a cada gravação do mesmo arquivo: o
 * mesmo arquivo só reabre depois deste intervalo. */
const EDITOR_PAUSE = 4000;
let lastOpened: { path: string; at: number } | null = null;

function followInEditor(chatId: string, change: LiveChange) {
  const { editor } = useLive.getState();
  if (!editor || change.kind === "removed" || useWorkspace.getState().activeChatId !== chatId) return;
  const now = Date.now();
  if (lastOpened && lastOpened.path === change.path && now - lastOpened.at < EDITOR_PAUSE) return;
  lastOpened = { path: change.path, at: now };
  void openLiveInEditor(chatId, change.path, editor).catch(reportError);
}

export function connectLive() {
  const off = onCore("live-file", ({ chatId, turnId, file }) => {
    useLive.setState((state) => {
      const current = state.chats[chatId];
      // Um pedido novo começou a olhar a pasta: a lista recomeça.
      if (!file) return { chats: { ...state.chats, [chatId]: { turnId, files: [] } }, selected: { ...state.selected, [chatId]: null } };
      const files = withChange(current?.turnId === turnId ? current.files : [], file);
      // O temporário que sumiu sai da lista sem abrir o painel nem roubar a vez.
      if (file.kind === "discarded") {
        const selected = state.selected[chatId] === file.path ? { ...state.selected, [chatId]: null } : state.selected;
        return { chats: { ...state.chats, [chatId]: { turnId, files } }, selected };
      }
      return {
        chats: { ...state.chats, [chatId]: { turnId, files } },
        panel: chatId in state.panel ? state.panel : { ...state.panel, [chatId]: true },
        selected: state.follow || !state.selected[chatId] ? { ...state.selected, [chatId]: file.path } : state.selected,
      };
    });
    if (file && file.kind !== "discarded") followInEditor(chatId, { ...file, kind: file.kind });
  });
  return () => void off.then((unlisten) => unlisten());
}
