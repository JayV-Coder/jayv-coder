import type { Chat, TurnStatus } from "@/modules/core";

/** O que vira notificação. As `org.*` são da conta: o banco grava (ver a
 * migração `notifications` do repositório `JayV-Coder/supabase`) e chegam em
 * qualquer aparelho. As outras são deste aparelho: a tela as percebe sozinha
 * e elas vivem enquanto o app está aberto. */
export type NotificationKind =
  | "org.invited" | "org.inviteAccepted" | "org.inviteDeclined" | "org.roleChanged" | "org.removed" | "org.deleted" | "org.policyChanged"
  | "turn.answered" | "turn.asking" | "turn.failed" | "turn.blocked"
  | "quota.crossed";

export const ACCOUNT_KINDS: NotificationKind[] = ["org.invited", "org.inviteAccepted", "org.inviteDeclined", "org.roleChanged", "org.removed", "org.deleted", "org.policyChanged"];

/** Os nomes do momento em que aconteceu: a frase é montada na hora de ler, no
 * idioma de quem lê. */
export type NotificationData = Record<string, string | number | null | undefined>;

export interface AppNotification {
  id: string;
  kind: NotificationKind;
  data: NotificationData;
  createdAt: string;
  read: boolean;
  /** Deste aparelho (não está no banco). */
  local: boolean;
}

/** Onde a notificação leva quando é clicada. */
export type NotificationTarget =
  | { kind: "chat"; chatId: string }
  | { kind: "organization"; orgId: string }
  | { kind: "organizations" }
  | { kind: "stats" }
  | null;

export function targetOf(notification: AppNotification): NotificationTarget {
  const { kind, data } = notification;
  if (kind.startsWith("turn.") && typeof data.chatId === "string") return { kind: "chat", chatId: data.chatId };
  if (kind === "quota.crossed") return { kind: "stats" };
  if (kind === "org.invited") return { kind: "organizations" };
  if (kind === "org.removed" || kind === "org.deleted") return null;
  if (typeof data.orgId === "string") return { kind: "organization", orgId: data.orgId };
  return null;
}

/** A cor do semáforo de cada tipo: o que pede ação é âmbar, o que deu errado é
 * vermelho, o que ficou pronto é verde; o resto é informação. */
export type Tone = "go" | "ask" | "stop" | "info";

export function toneOf(kind: NotificationKind): Tone {
  switch (kind) {
    case "turn.answered": case "org.inviteAccepted": return "go";
    case "turn.asking": case "org.invited": case "quota.crossed": return "ask";
    case "turn.failed": case "turn.blocked": case "org.removed": case "org.deleted": case "org.inviteDeclined": return "stop";
    default: return "info";
  }
}

const OPEN: TurnStatus[] = ["queued", "flying"];
const SETTLED: Partial<Record<TurnStatus, NotificationKind>> = { answered: "turn.answered", failed: "turn.failed", blocked: "turn.blocked" };

/** O que a tela já viu de cada turno e de cada pergunta, para notar só o que
 * mudou entre duas leituras do banco. */
export interface TurnMemory {
  statuses: Map<string, TurnStatus>;
  questions: Set<string>;
}

export const emptyMemory = (): TurnMemory => ({ statuses: new Map(), questions: new Set() });

export interface TurnEvent { kind: NotificationKind; chat: Chat; turnId: string }

const questionKey = (chat: Chat) => (chat.question ? `${chat.question.turnId}:${chat.question.code}` : null);

/** Compara a leitura nova com a anterior. A primeira leitura (`memory` vazia e
 * `first`) só aprende: o que já estava pronto ao abrir o app não é novidade.
 *
 * Um turno conta quando sai da fila (ou aparece já fechado); o turno que
 * terminou fazendo uma pergunta vira só a pergunta. */
export function turnEvents(memory: TurnMemory, chats: Chat[], first: boolean): TurnEvent[] {
  const events: TurnEvent[] = [];
  for (const chat of chats) {
    const asked = questionKey(chat);
    if (asked && !memory.questions.has(asked)) {
      memory.questions.add(asked);
      if (!first) events.push({ kind: "turn.asking", chat, turnId: chat.question!.turnId });
    }
    for (const turn of chat.turns) {
      const before = memory.statuses.get(turn.id);
      memory.statuses.set(turn.id, turn.status);
      const settled = SETTLED[turn.status];
      if (first || !settled) continue;
      if (before !== undefined && !OPEN.includes(before)) continue;
      if (turn.id === chat.question?.turnId) continue;
      events.push({ kind: settled, chat, turnId: turn.id });
    }
  }
  return events;
}

/** Quantas notificações deste aparelho ficam; as mais velhas saem. */
export const LOCAL_LIMIT = 50;

export function newestFirst(list: AppNotification[]) {
  return [...list].sort((a, b) => b.createdAt.localeCompare(a.createdAt));
}

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [["day", 86_400], ["hour", 3_600], ["minute", 60]];

/** "há 5 min", "ontem": quanto tempo faz, no idioma da tela. */
export function timeAgo(iso: string, locale: string, now = Date.now()) {
  const seconds = Math.round((new Date(iso).getTime() - now) / 1000);
  const format = new Intl.RelativeTimeFormat(locale, { numeric: "auto", style: "short" });
  for (const [unit, size] of UNITS) if (Math.abs(seconds) >= size) return format.format(Math.round(seconds / size), unit);
  return format.format(0, "second");
}
