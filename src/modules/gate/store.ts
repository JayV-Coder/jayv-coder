import { create } from "zustand";
import { bus, commands, onCore, type Aspect, type Chat, type EntryCheck, type ExitCheck, type GateFeed, type Project, type Tally } from "@/modules/core";
import { ENTRY_VERDICTS, EXIT_VERDICTS } from "@/modules/conversation";
import type { Key } from "@/modules/i18n";

export const TALLY: [keyof Tally, Aspect, Key][] = [
  ["passed", "go", "gate.tally.passed"],
  ["asked", "ask", "gate.tally.asked"],
  ["blocked", "stop", "gate.tally.blocked"],
  ["held", "stop", "gate.tally.held"],
];

/** O tipo de saída vem do núcleo em português; é por ele que se acha o nome e
 * o ícone. */
/** Os tipos de saída. `comando` e `arquivo` são a grafia dos checks gravados
 * antes da troca para identificadores em inglês. */
export const EXIT_KINDS: Record<string, Key> = { command: "exit.kind.command", file: "exit.kind.file", "comando": "exit.kind.command", "arquivo": "exit.kind.file" };

const VERDICT_TALLY: Record<EntryCheck["verdict"], keyof Tally> = { pass: "passed", ask: "asked", block: "blocked" };
const EMPTY: GateFeed = { entries: [], exits: [], tally: { passed: 0, asked: 0, blocked: 0, held: 0 } };

interface GateState {
  feed: GateFeed;
  /** O projeto aberto: a portaria só mostra o que passou pelos chats dele. */
  project: Project | null;
  chats: Record<string, Chat>;
  /** Cartões recém-chegados, que acendem a lente uma vez ao entrar. */
  arriving: Record<string, true>;
}

export const useGate = create<GateState>(() => ({ feed: EMPTY, project: null, chats: {}, arriving: {} }));

export async function loadGate() {
  try {
    const asked = useGate.getState().project?.id ?? null;
    const feed = await commands.gateFeed(asked);
    // A resposta lenta de outro projeto não pinta o placar deste.
    if ((useGate.getState().project?.id ?? null) === asked) useGate.setState({ feed });
  } catch (error) {
    console.error(error);
  }
}

export function settleArrival(id: string) {
  useGate.setState((state) => {
    const { [id]: _, ...arriving } = state.arriving;
    return { arriving };
  });
}

/** O último registro da portaria naquele chat: a saída, quando houve alguma, ou
 * o pedido que entrou. É o que os cartões de chat mostram sem abrir a portaria.
 * O feed vem do banco, então um chat parado há semanas continua contando o que
 * passou por ele. */
export type GatePassInfo = { aspect: Aspect; kind: Key | null; rawKind: string | null; verdict: Key; detail: string; at: string };

export function lastGatePass(feed: GateFeed, chatId: string): GatePassInfo | null {
  const exit = feed.exits.find((check) => check.chatId === chatId);
  const entry = feed.entries.find((check) => check.chatId === chatId);
  if (exit && (!entry || new Date(exit.at) >= new Date(entry.at))) {
    const { aspect, label } = EXIT_VERDICTS[exit.verdict] ?? EXIT_VERDICTS.held;
    return { aspect, kind: EXIT_KINDS[exit.kind] ?? null, rawKind: exit.kind, verdict: label, detail: exit.target, at: exit.at };
  }
  if (!entry) return null;
  const { aspect, label } = ENTRY_VERDICTS[entry.verdict] ?? ENTRY_VERDICTS.block;
  return { aspect, kind: null, rawKind: null, verdict: label, detail: entry.prompt, at: entry.at };
}

function mine(chatId: string) {
  const { project, chats } = useGate.getState();
  return project !== null && chatId in chats;
}

function arrived(ids: string[]) {
  useGate.setState((state) => ({ arriving: { ...state.arriving, ...Object.fromEntries(ids.map((id) => [id, true as const])) } }));
}

/** A portaria acompanha o projeto aberto e se relê quando a vista dela, ou a
 * grade de chats que cita o último registro, aparece. */
export function connectGate() {
  const offs = [
    onCore("gate-entry", ({ check }) => {
      if (!mine(check.chatId)) return;
      const { feed } = useGate.getState();
      // Reenvio: o pedido já tem cartão aqui. Ele é atualizado, não duplicado, e
      // o placar vem do banco para não contar duas vezes a mesma tentativa.
      if (feed.entries.some((known) => known.id === check.id)) { void loadGate(); return; }
      const key = VERDICT_TALLY[check.verdict];
      useGate.setState({ feed: { ...feed, entries: [check, ...feed.entries], tally: { ...feed.tally, [key]: feed.tally[key] + 1 } } });
      arrived([check.id]);
    }),
    onCore("gate-exit", ({ checks }) => {
      const ours: ExitCheck[] = checks.filter((check) => mine(check.chatId));
      if (ours.length === 0) return;
      const { feed } = useGate.getState();
      if (ours.some((check) => feed.exits.some((known) => known.turnId === check.turnId))) { void loadGate(); return; }
      const held = ours.filter((check) => check.verdict === "held").length;
      useGate.setState({ feed: { ...feed, exits: [...ours, ...feed.exits], tally: { ...feed.tally, held: feed.tally.held + held } } });
      arrived(ours.map((check) => check.id));
    }),
  ];
  const offScope = bus.on("scope:changed", ({ project, chats }) => {
    const changed = useGate.getState().project?.id !== project?.id;
    useGate.setState({ project, chats: Object.fromEntries(chats.map((chat) => [chat.id, chat])), ...(changed ? { feed: EMPTY } : {}) });
    // O placar da barra de status vale para o projeto aberto por qualquer
    // caminho (notificação, chat da organização), não só pela página de chats.
    if (changed && project) void loadGate();
  });
  const offView = bus.on("view:changed", ({ view }) => {
    if (view === "gate" || view === "chats") void loadGate();
  });
  return () => {
    offScope();
    offView();
    offs.forEach((off) => void off.then((unlisten) => unlisten()));
  };
}
