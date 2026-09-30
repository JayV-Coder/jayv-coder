import { create } from "zustand";
import { bus, commands, onCore, type Activity, type Question, type TurnView } from "@/modules/core";
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
}

interface ConversationState {
  live: Record<string, Live>;
  answering: Answering;
  /** O texto ainda não enviado, por chat. */
  drafts: Record<string, string>;
}

const IDLE: Answering = { turnId: null, writing: false, picked: [] };

export const useConversation = create<ConversationState>(() => ({ live: {}, answering: IDLE, drafts: {} }));

/** O que se sabe deste pedido, juntando o que o banco gravou com o que chegou
 * pelo barramento. O banco escreve o rascunho com folga, então o que está na
 * tela pode estar à frente dele: fica o mais longo dos dois, nunca a soma —
 * somar escreveria a resposta duas vezes. */
export function liveOf(turn: TurnView, live: Record<string, Live>) {
  const known = live[turn.id] ?? { text: "", beats: {} };
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
  try {
    await commands.enqueuePrompt(value, chatId, turnId);
    bus.emit("prompt:sent", { chatId });
  } catch (error) {
    reportError(error);
    if (!turnId && !useConversation.getState().drafts[chatId]?.trim()) setDraft(chatId, value);
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

export async function clearChat(chatId: string) {
  try {
    await commands.clearChat(chatId);
    bus.emit("prompt:sent", { chatId });
  } catch (error) {
    reportError(error);
  }
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
    onCore("turn-chunk", ({ turnId, text }) => {
      useConversation.setState((state) => {
        const live = state.live[turnId] ?? { text: "", beats: {} };
        return { live: { ...state.live, [turnId]: { ...live, text: live.text + text } } };
      });
    }),
    onCore("turn-beat", ({ turnId, seq, kind, detail }) => {
      useConversation.setState((state) => {
        const live = state.live[turnId] ?? { text: "", beats: {} };
        const beat: Activity = { at: new Date().toISOString(), seq, kind, detail };
        return { live: { ...state.live, [turnId]: { ...live, beats: { ...live.beats, [seq]: beat } } } };
      });
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
