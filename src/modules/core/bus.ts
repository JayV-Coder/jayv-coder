import type { Chat, Project } from "./types";

/** O barramento entre os módulos da tela. Nenhum módulo mexe no estado de
 * outro: quem muda algo avisa por aqui, e quem se importa reage. É isto que
 * deixa cada módulo trocável sem abrir os outros. */
export interface BusEvents {
  /** O projeto aberto ou os chats dele mudaram. */
  "scope:changed": { project: Project | null; chats: Chat[] };
  /** O retrato do banco foi relido. */
  "workspace:loaded": { chats: Chat[] };
  /** A tela trocou de vista. */
  "view:changed": { view: View };
  /** Um pedido foi entregue ao núcleo, ou uma pergunta respondida. */
  "prompt:sent": { chatId: string };
  /** A configuração dos provedores foi salva. */
  "settings:saved": Record<string, never>;
}

export type View = "projects" | "chats" | "chat" | "gate" | "status" | "stats" | "settings" | "profile" | "organizations" | "organization";

type Handler<K extends keyof BusEvents> = (payload: BusEvents[K]) => void;
const handlers = new Map<keyof BusEvents, Set<Handler<never>>>();

export const bus = {
  on<K extends keyof BusEvents>(event: K, handler: Handler<K>): () => void {
    const set = handlers.get(event) ?? new Set();
    set.add(handler as Handler<never>);
    handlers.set(event, set);
    return () => set.delete(handler as Handler<never>);
  },
  emit<K extends keyof BusEvents>(event: K, payload: BusEvents[K]) {
    handlers.get(event)?.forEach((handler) => (handler as Handler<K>)(payload));
  },
};
