import { useEffect, useRef, type FormEvent, type KeyboardEvent } from "react";
import type { Chat } from "@/modules/core";
import { answerQuestion, answeringFor, sendPrompt, setDraft, useConversation } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { Button } from "@/components/ui/button";
import { AskingPanel } from "./AskingPanel";
import { PendingBanner } from "./PendingBanner";

/** A caixa de enviar mensagem. É o banco que escolhe o traje dela: sem
 * pergunta em aberto, a caixa de sempre; com pergunta, o painel da pergunta e
 * o texto travado até o desenvolvedor pedir para escrever.
 *
 * O botão só cai quando não há chat. Desligá-lo enquanto um pedido roda era o
 * que empurrava o desenvolvedor a mandar por cima e ver o texto sumir; com
 * fila, mandar em cima da espera é o comportamento normal. */
export function Composer({ chat }: { chat: Chat | null }) {
  const t = useT();
  const input = useRef<HTMLTextAreaElement>(null);
  const draft = useConversation((state) => (chat ? state.drafts[chat.id] ?? "" : ""));
  const answering = useConversation((state) => state.answering);
  const question = chat?.question ?? null;
  const writing = question ? answeringFor(question, answering).writing : false;
  const locked = !chat || (question !== null && !writing);

  useEffect(() => {
    const element = input.current;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${Math.min(element.scrollHeight, 180)}px`;
  }, [draft]);

  useEffect(() => {
    if (!locked) input.current?.focus();
  }, [locked, chat?.id]);

  const submit = async (event?: FormEvent) => {
    event?.preventDefault();
    const value = draft.trim();
    if (!value || !chat) return;
    setDraft(chat.id, "");
    // Com pergunta em aberto, o que foi escrito é a resposta dela: é o caminho
    // do `RESPONDER`, e o texto livre vale para qualquer tipo de pergunta.
    if (question && writing) await answerQuestion(question, chat.id, [], value);
    else await sendPrompt(value, chat.id);
    input.current?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      void submit();
    }
  };

  return (
    <form onSubmit={submit} className="mx-[max(40px,calc((100%-880px)/2))] mb-6 overflow-hidden rounded-lg border border-border bg-card shadow-sm transition-[border-color,box-shadow] focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/20">
      <PendingBanner chat={chat} />
      {chat && question && <AskingPanel chat={chat} question={question} />}
      <textarea
        ref={input}
        rows={1}
        value={draft}
        disabled={locked}
        onChange={(event) => chat && setDraft(chat.id, event.target.value)}
        onKeyDown={onKeyDown}
        placeholder={t(!question ? "composer.placeholder.open" : writing ? "composer.placeholder.writing" : "composer.placeholder.locked")}
        className="block max-h-[180px] w-full resize-none bg-transparent px-4 pt-3.5 pb-2 leading-relaxed outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-60"
      />
      <div className="flex items-center justify-between border-t border-border py-2 pe-2 ps-4">
        <small className="text-caption text-muted-foreground">{t(!question ? "composer.hint.open" : writing ? "composer.hint.writing" : "composer.hint.locked")}</small>
        {(!question || writing) && <Button type="submit" size="sm" disabled={!chat}>{t("composer.send")}</Button>}
      </div>
    </form>
  );
}
