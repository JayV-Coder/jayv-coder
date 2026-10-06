import { create } from "zustand";
import { bus, commands, onCore, NO_GRANTS, type Activity, type FormItem, type Grants, type Question, type TurnView } from "@/modules/core";
import { reportError } from "@/modules/feedback";

/** O que está acontecendo agora, por pedido em aberto: o texto que vai
 * chegando e as etapas já anunciadas. Não é a verdade — a verdade está no
 * banco, e é de lá que a conversa é redesenhada. É a ponte entre dois retratos
 * do banco, para que a espera não seja um silêncio. */
interface Live {
  text: string;
  beats: Record<number, Activity>;
}

/** Qual pergunta está aberta na caixa, se o desenvolvedor pediu para escrever
 * em vez de clicar, e o que ele marcou e ainda não enviou. Nada disto é
 * verdade guardada: a pergunta e o desfecho dela estão no banco. Isto é
 * rascunho, como o texto ainda não enviado. */
interface Answering {
  turnId: string | null;
  writing: boolean;
  picked: string[];
  /** No formulário, a resposta de cada pergunta, na ordem delas. */
  form: string[];
  /** No formulário, a pergunta da vez: uma por etapa. */
  step: number;
  /** A pergunta recolhida numa linha, para o chat ter espaço. */
  folded: boolean;
}

interface ConversationState {
  live: Record<string, Live>;
  answering: Answering;
  /** O texto ainda não enviado, por chat. */
  drafts: Record<string, string>;
  /** As permissões escolhidas para o próximo pedido, por chat. Valem para um
   * envio só e somem com ele. */
  grants: Record<string, Grants>;
}

const IDLE: Answering = { turnId: null, writing: false, picked: [], form: [], step: 0, folded: false };

export const useConversation = create<ConversationState>(() => ({ live: {}, answering: IDLE, drafts: {}, grants: {} }));

export function grantsOf(chatId: string | null | undefined, grants: Record<string, Grants>): Grants {
  return (chatId && grants[chatId]) || NO_GRANTS;
}

export function hasGrants(grants: Grants): boolean {
  return grants.shell || grants.git || grants.network || grants.commands.length > 0;
}

export function setGrants(chatId: string, grants: Grants) {
  useConversation.setState((state) => ({ grants: { ...state.grants, [chatId]: grants } }));
}

/** O que se sabe deste pedido, juntando o que o banco gravou com o que chegou
 * pelo barramento. O banco escreve o rascunho com folga, então o que está na
 * tela pode estar à frente dele: fica o mais longo dos dois, nunca a soma —
 * somar escreveria a resposta duas vezes. */
export function liveOf(turn: TurnView, live: Live | undefined) {
  const known = live ?? { text: "", beats: {} };
  const partial = turn.partial ?? "";
  const text = partial.length > known.text.length ? partial : known.text;
  const beats: Record<number, Activity> = { ...known.beats };
  (turn.activity ?? []).forEach((row) => { if (!beats[row.seq]) beats[row.seq] = row; });
  return { text, beats: Object.values(beats).sort((a, b) => a.seq - b.seq) };
}

/** O rascunho da resposta, só se ele pertencer à pergunta que está aberta. Uma
 * pergunta nova começa sem nada marcado. */
export function answeringFor(question: Question | null, answering: Answering): Answering {
  return question && answering.turnId === question.turnId ? answering : { ...IDLE, turnId: question?.turnId ?? null };
}

export function setDraft(chatId: string, value: string) {
  useConversation.setState((state) => ({ drafts: { ...state.drafts, [chatId]: value } }));
}

export function setWriting(question: Question, writing: boolean) {
  useConversation.setState((state) => ({ answering: { ...answeringFor(question, state.answering), writing } }));
}

/** Vai para outra pergunta do formulário. O que já foi escrito nas outras
 * fica: só se envia tudo junto, na última etapa. */
export function setStep(question: Question, step: number) {
  const last = Math.max(0, formItems(question).length - 1);
  useConversation.setState((state) => ({ answering: { ...answeringFor(question, state.answering), step: Math.min(Math.max(0, step), last) } }));
}

export function setFolded(question: Question, folded: boolean) {
  useConversation.setState((state) => ({ answering: { ...answeringFor(question, state.answering), folded } }));
}

/** As perguntas do formulário. Uma linha que não se lê fica de fora, e o
 * `RESPONDER` continua de pé. */
export function formItems(question: Question): FormItem[] {
  return question.options.flatMap((item) => {
    try {
      const parsed = JSON.parse(item) as FormItem;
      return typeof parsed.prompt === "string" && Array.isArray(parsed.options) ? [parsed] : [];
    } catch {
      return [];
    }
  });
}

export function answerForm(question: Question, index: number, value: string) {
  useConversation.setState((state) => {
    const current = answeringFor(question, state.answering);
    const form = [...current.form];
    form[index] = value;
    return { answering: { ...current, form } };
  });
}

export function pick(question: Question, option: string, checked: boolean) {
  useConversation.setState((state) => {
    const current = answeringFor(question, state.answering);
    const picked = question.kind === "multiple"
      ? checked ? [...new Set([...current.picked, option])] : current.picked.filter((item) => item !== option)
      : checked ? [option] : [];
    return { answering: { ...current, picked } };
  });
}

/** Entrega o pedido ao banco e volta. Não espera resposta de modelo nenhum:
 * quando esta função retorna, o que o desenvolvedor escreveu já é uma linha no
 * disco com número próprio, e a tela o desenha lendo de lá. Mandar outra coisa
 * em seguida não atropela nada — o pedido novo entra na fila atrás do anterior.
 * Se a gravação falhar, o texto volta para a caixa: perder o que foi digitado é
 * pior do que qualquer erro na tela. */
export async function sendPrompt(value: string, chatId: string, turnId: string | null = null) {
  // As permissões do seletor vão com este pedido e saem da caixa; o reenvio
  // de um turno mantém as que ele já tinha.
  const grants = turnId ? NO_GRANTS : grantsOf(chatId, useConversation.getState().grants);
  const chosen = hasGrants(grants);
  if (chosen) setGrants(chatId, NO_GRANTS);
  try {
    await commands.enqueuePrompt(value, chatId, turnId, chosen ? grants : null);
    bus.emit("prompt:sent", { chatId });
  } catch (error) {
    reportError(error);
    if (!turnId && !useConversation.getState().drafts[chatId]?.trim()) setDraft(chatId, value);
    if (chosen) setGrants(chatId, grants);
  }
}

/** A resposta volta pelo mesmo caminho de qualquer pedido: o núcleo compõe o
 * texto — a tela não escolhe outras palavras para o que foi clicado —, a
 * Portaria pontua o par pergunta/resposta e só então o modelo é chamado. */
export async function answerQuestion(question: Question, chatId: string, picked: string[], text: string | null = null) {
  try {
    await commands.answerQuestion(question.turnId, picked, text);
    useConversation.setState({ answering: IDLE });
    bus.emit("prompt:sent", { chatId });
  } catch (error) {
    reportError(error);
    // Como no pedido: a resposta digitada volta para a caixa.
    if (text && !useConversation.getState().drafts[chatId]?.trim()) setDraft(chatId, text);
  }
}

export async function dismissQuestion(question: Question, chatId: string) {
  try {
    await commands.dismissQuestion(question.turnId);
    bus.emit("prompt:sent", { chatId });
  } catch (error) {
    reportError(error);
  }
}

/** Para o pedido: o que está no ar cai junto com o agente; o que espera na
 * fila sai dela. Os dois ficam no chat como falhos, com o reenvio. */
export async function cancelTurn(turnId: string, chatId: string) {
  try {
    await commands.cancelTurn(turnId);
    bus.emit("prompt:sent", { chatId });
  } catch (error) {
    reportError(error);
  }
}

export async function clearChat(chatId: string) {
  try {
    await commands.clearChat(chatId);
    bus.emit("prompt:sent", { chatId });
  } catch (error) {
    reportError(error);
  }
}

/** O que chegou do núcleo e ainda não foi para a tela. Os pedaços e as etapas
 * de um quadro entram juntos, num `setState` só: com o Claude transmitindo, um
 * redesenho por evento engasgava a conversa. */
let waiting: Array<{ turnId: string; apply: (live: Live) => Live }> = [];
let scheduled = false;

function queue(turnId: string, apply: (live: Live) => Live) {
  waiting.push({ turnId, apply });
  if (scheduled) return;
  scheduled = true;
  const flush = () => {
    scheduled = false;
    const batch = waiting;
    waiting = [];
    useConversation.setState((state) => {
      const live = { ...state.live };
      for (const { turnId: id, apply: change } of batch) live[id] = change(live[id] ?? { text: "", beats: {} });
      return { live };
    });
  };
  if (typeof requestAnimationFrame === "function" && typeof document !== "undefined" && !document.hidden) requestAnimationFrame(flush);
  else setTimeout(flush, 16);
}

/** O pedido contado enquanto acontece. Estes dois avisos não redesenham a
 * conversa: eles mexem só no balão em aberto. O redesenho continua sendo do
 * `turn-settled`, quando há o que redesenhar.
 *
 * O pedido que fechou não tem mais o que acompanhar: a resposta dele está em
 * `messages`, gravada. Guardar o rascunho depois disso é guardar duas versões
 * do mesmo texto, e uma delas envelhece. */
export function connectConversation() {
  const offs = [
    onCore("turn-chunk", ({ turnId, text }) => queue(turnId, (live) => ({ ...live, text: live.text + text }))),
    onCore("turn-beat", ({ turnId, seq, kind, detail }) => {
      const beat: Activity = { at: new Date().toISOString(), seq, kind, detail };
      queue(turnId, (live) => ({ ...live, beats: { ...live.beats, [seq]: beat } }));
    }),
  ];
  const offLoaded = bus.on("workspace:loaded", ({ chats }) => {
    const open = new Set(chats.flatMap((chat) => chat.turns.filter((turn) => turn.status === "queued" || turn.status === "flying").map((turn) => turn.id)));
    useConversation.setState((state) => ({ live: Object.fromEntries(Object.entries(state.live).filter(([turnId]) => open.has(turnId))) }));
  });
  return () => {
    offLoaded();
    offs.forEach((off) => void off.then((unlisten) => unlisten()));
  };
}
