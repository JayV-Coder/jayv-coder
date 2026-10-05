import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { ArrowUpIcon, SquareIcon } from "lucide-react";
import { WORK_MODES, type Chat, type WorkMode } from "@/modules/core";
import { answerQuestion, answeringFor, cancelTurn, sendPrompt, setDraft, useConversation } from "@/modules/conversation";
import { notify } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { MOD, modeCommand, shortcutText } from "@/modules/commands";
import { openTurns, setWorkMode } from "@/modules/workspace";
import { Kbd } from "@/components/atoms";
import { SegmentedControl } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { AskingPanel } from "./AskingPanel";

/** A caixa de enviar mensagem, como o prompt de um terminal: `❯`, o cursor
 * verde-limão e os atalhos à vista embaixo. É o banco que escolhe o traje dela: sem
 * pergunta em aberto, a caixa de sempre; com pergunta, o painel da pergunta e
 * o texto travado até o desenvolvedor pedir para escrever.
 *
 * O botão de enviar cede o lugar ao `Parar` enquanto o JayV de fato trabalha
 * (o pedido no ar, não o que espera na fila). Mandar por cima continua
 * valendo com a tecla Enter: o texto entra na fila, e não some. Fora disso o
 * botão só fica apagado enquanto não há o que mandar.
 * O andamento do pedido mora no painel da esquerda (`ProgressPanel`). */
export function Composer({ chat }: { chat: Chat | null }) {
  const t = useT();
  const input = useRef<HTMLTextAreaElement>(null);
  const draft = useConversation((state) => (chat ? state.drafts[chat.id] ?? "" : ""));
  const answering = useConversation((state) => state.answering);
  const question = chat?.question ?? null;
  const writing = question ? answeringFor(question, answering).writing : false;
  const locked = !chat || (question !== null && !writing);
  // O pedido que já recebeu o "Parar": o botão não manda de novo enquanto o
  // agente cai.
  const [stopping, setStopping] = useState<string | null>(null);
  const flying = openTurns(chat).find((turn) => turn.status === "flying") ?? null;

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
    let value = draft.trim();
    if (!value || !chat) return;
    setDraft(chat.id, "");
    // `/plan`, `/build` e `/auto` trocam o modo do chat; o que vier depois do
    // comando segue como pedido, já no modo novo.
    const command = !question ? modeCommand(value) : null;
    if (command) {
      // Modo recusado: nada de aviso de sucesso nem de pedido no modo antigo.
      if (!(await setWorkMode(chat.id, command.mode))) {
        setDraft(chat.id, draft);
        return;
      }
      if (!command.rest) {
        notify(t("mode.set", { name: t(`mode.${command.mode}`) }));
        input.current?.focus();
        return;
      }
      value = command.rest;
    }
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
              {!question && <span className="inline-flex items-center gap-1.5"><Kbd>/plan</Kbd><Kbd>/build</Kbd><Kbd>/auto</Kbd>{t("composer.key.mode")}</span>}
            </small>
          )}
        {chat && (
          <SegmentedControl<WorkMode>
            label={`${t("mode.label")} (${shortcutText("workMode")})`}
            value={chat.workMode ?? "auto"}
            options={WORK_MODES.map((mode) => ({ value: mode, label: t(`mode.${mode}`), hint: t(`mode.${mode}.hint`) }))}
            onChange={(mode) => void setWorkMode(chat.id, mode)}
            className="[&_button]:h-6 [&_button]:px-2"
          />
        )}
        {flying && chat ? (
          <Button
            type="button"
            variant="destructive"
            size="icon-sm"
            disabled={stopping === flying.id}
            aria-label={stopping === flying.id ? t("pending.stopping") : t("pending.stop")}
            title={t("pending.stop.title")}
            onClick={() => { setStopping(flying.id); void cancelTurn(flying.id, chat.id); }}
          >
            <SquareIcon aria-hidden="true" className="fill-current" />
          </Button>
        ) : (!question || writing) && (
          <Button type="submit" size="icon-sm" disabled={!chat || !draft.trim()} aria-label={t("composer.send")} title={t("composer.send")}>
            <ArrowUpIcon aria-hidden="true" />
          </Button>
        )}
      </div>
    </form>
  );
}
