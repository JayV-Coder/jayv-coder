import { useEffect, useRef, type FormEvent, type KeyboardEvent } from "react";
import { ArrowUpIcon } from "lucide-react";
import type { Chat } from "@/modules/core";
import { answerQuestion, answeringFor, sendPrompt, setDraft, useConversation } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { MOD } from "@/modules/commands";
import { Kbd } from "@/components/atoms";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { AskingPanel } from "./AskingPanel";
import { PendingBanner } from "./PendingBanner";

/** A caixa de enviar mensagem, como o prompt de um terminal: `❯`, o cursor
 * verde-limão e os atalhos à vista embaixo. É o banco que escolhe o traje dela: sem
 * pergunta em aberto, a caixa de sempre; com pergunta, o painel da pergunta e
 * o texto travado até o desenvolvedor pedir para escrever.
 *
 * O botão só cai quando não há chat. Desligá-lo enquanto um pedido roda era o
 * que empurrava o desenvolvedor a mandar por cima e ver o texto sumir; com
 * fila, mandar em cima da espera é o comportamento normal. Ele só fica
 * apagado enquanto não há o que mandar. */
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
    element.style.height = `${Math.min(element.scrollHeight, 220)}px`;
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
    <form onSubmit={submit} className="overflow-hidden rounded-md border border-border bg-card font-mono transition-[border-color,box-shadow] focus-within:border-accent focus-within:ring-3 focus-within:ring-accent/25">
      <PendingBanner chat={chat} />
      {chat && question && <AskingPanel chat={chat} question={question} />}
      <div className="flex items-start">
      <span aria-hidden="true" className={cn("ps-3.5 pt-3 font-semibold text-go", locked && "text-faint")}>❯</span>
      <textarea
        ref={input}
        rows={1}
        value={draft}
        disabled={locked}
        onChange={(event) => chat && setDraft(chat.id, event.target.value)}
        onKeyDown={onKeyDown}
        placeholder={t(!question ? "composer.placeholder.open" : writing ? "composer.placeholder.writing" : "composer.placeholder.locked")}
        className="block max-h-[220px] min-h-11 w-full resize-none bg-transparent ps-2.5 pe-4 pt-3 pb-1 text-sm leading-relaxed caret-accent outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-60"
      />
      </div>
      <div className="flex items-center gap-3 border-t border-dashed border-border py-1.5 pe-1.5 ps-3.5">
        {/* Os atalhos à vista, como no prompt do Warp. */}
        {locked
          ? <small className="me-auto text-caption text-muted-foreground">{t("composer.hint.locked")}</small>
          : (
            <small className="me-auto flex flex-wrap items-center gap-x-3.5 gap-y-1 text-caption text-muted-foreground">
              <span className="inline-flex items-center gap-1.5"><Kbd>↵</Kbd>{t(writing ? "composer.key.answer" : "composer.key.send")}</span>
              <span className="inline-flex items-center gap-1.5"><Kbd>⇧</Kbd><Kbd>↵</Kbd>{t("composer.key.newline")}</span>
              <span className="inline-flex items-center gap-1.5"><Kbd>{MOD}</Kbd><Kbd>K</Kbd>{t("palette.title")}</span>
            </small>
          )}
        {(!question || writing) && (
          <Button type="submit" size="icon-sm" disabled={!chat || !draft.trim()} aria-label={t("composer.send")} title={t("composer.send")}>
            <ArrowUpIcon aria-hidden="true" />
          </Button>
        )}
      </div>
    </form>
  );
}
