import { useEffect, useRef, useState } from "react";
import { ArrowDownIcon } from "lucide-react";
import type { Chat, Project } from "@/modules/core";
import { sendPrompt, useConversation } from "@/modules/conversation";
import { useLocale, useT } from "@/modules/i18n";
import { formatCost, formatDuration, formatTokens, useUsage } from "@/modules/usage";
import { openFile, openTurns } from "@/modules/workspace";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { MessageBubble } from "./MessageBubble";
import { PendingBubble } from "./PendingBubble";
import { Welcome } from "./Welcome";

/** Quanto do fim ainda conta como "no fim": a folga de uma linha de texto. */
const NEAR_BOTTOM = 96;

/** A conversa inteira: o que está gravado e, no fim, os pedidos em aberto.
 *
 * A rolagem segue a resposta só enquanto quem lê está no fim. Quem subiu para
 * reler um trecho fica onde está, e um botão leva de volta ao fim. */
export function Timeline({ chat, project }: { chat: Chat | null; project: Project | null }) {
  const scroller = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const [away, setAway] = useState(false);
  const t = useT();
  const locale = useLocale();
  const live = useConversation((state) => state.live);
  const spent = useUsage((state) => (chat ? state.turns[chat.id] : undefined));
  // O rodapé da resposta: os tokens e o tempo do turno, com `≈` quando a
  // ferramenta não os informou.
  const meta = (turnId: string) => {
    const usage = spent?.[turnId];
    if (!usage) return undefined;
    const parts = [t("usage.turn.tokens", { input: formatTokens(usage.inputTokens, locale), output: formatTokens(usage.outputTokens, locale) })];
    if (usage.costUsd !== null) parts.push(formatCost(usage.costUsd, locale));
    if (usage.durationMs > 0) parts.push(formatDuration(usage.durationMs, locale));
    return `${usage.estimated ? "≈ " : ""}${parts.join(" · ")}`;
  };
  const turns = new Map((chat?.turns ?? []).map((turn) => [turn.id, turn]));
  const open = openTurns(chat);
  const sent = chat?.messages.filter((message) => message.role === "user").length ?? 0;

  const toBottom = (smooth = false) => {
    const element = scroller.current;
    if (!element) return;
    element.scrollTo({ top: element.scrollHeight, behavior: smooth ? "smooth" : "auto" });
    pinned.current = true;
    setAway(false);
  };

  // Abrir outro chat ou mandar um pedido novo volta ao fim: é lá que a
  // resposta vai aparecer.
  useEffect(() => toBottom(), [chat?.id, sent, open.length]);

  useEffect(() => {
    if (pinned.current) toBottom();
  }, [chat, live]);

  const onScroll = () => {
    const element = scroller.current;
    if (!element) return;
    const near = element.scrollHeight - element.scrollTop - element.clientHeight < NEAR_BOTTOM;
    pinned.current = near;
    setAway(!near);
  };

  const retry = (turnId: string) => {
    const said = chat?.messages.find((message) => message.turnId === turnId && message.role === "user");
    if (chat && said) void sendPrompt(said.content, chat.id, turnId);
  };

  const empty = !chat || chat.messages.length === 0;
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div ref={scroller} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto">
        <div className={cn("mx-auto min-h-full w-full max-w-3xl px-6 pt-6 pb-4", empty && "flex flex-col")}>
          {empty ? <Welcome project={project} chat={chat} /> : (
            <>
              {chat.messages.map((message, index) => {
                const turn = message.turnId ? turns.get(message.turnId) ?? null : null;
                return (
                  <MessageBubble
                    key={`${message.turnId ?? "m"}-${message.role}-${index}`}
                    role={message.role}
                    content={message.content}
                    turn={turn}
                    meta={message.role === "assistant" && message.turnId ? meta(message.turnId) : undefined}
                    onRetry={turn ? () => retry(turn.id) : undefined}
                    onOpenFile={project?.rootPath ? (path) => void openFile(chat.id, path) : undefined}
                  />
                );
              })}
              {open.map((turn, index) => <PendingBubble key={turn.id} turn={turn} place={index + 1} />)}
            </>
          )}
        </div>
      </div>
      {away && !empty && (
        <Button
          variant="outline"
          size="sm"
          onClick={() => toBottom(true)}
          className="absolute bottom-3 left-1/2 -translate-x-1/2 rounded-full bg-card shadow-md animate-pending-in motion-reduce:animate-none"
        >
          <ArrowDownIcon aria-hidden="true" />
          {t("chat.jumpToLatest")}
        </Button>
      )}
    </div>
  );
}
