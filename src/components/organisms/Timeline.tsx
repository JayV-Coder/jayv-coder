import { useEffect, useRef } from "react";
import type { Chat, Project } from "@/modules/core";
import { sendPrompt, useConversation } from "@/modules/conversation";
import { useLocale, useT } from "@/modules/i18n";
import { formatCost, formatDuration, formatTokens, useUsage } from "@/modules/usage";
import { openFile, openTurns } from "@/modules/workspace";
import { MessageBubble } from "./MessageBubble";
import { PendingBubble } from "./PendingBubble";
import { Welcome } from "./Welcome";

/** A conversa inteira: o que está gravado e, no fim, os pedidos em aberto. */
export function Timeline({ chat, project }: { chat: Chat | null; project: Project | null }) {
  const scroller = useRef<HTMLDivElement>(null);
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

  useEffect(() => {
    const element = scroller.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [chat, live]);

  const retry = (turnId: string) => {
    const said = chat?.messages.find((message) => message.turnId === turnId && message.role === "user");
    if (chat && said) void sendPrompt(said.content, chat.id, turnId);
  };

  return (
    <div ref={scroller} className="flex-1 overflow-auto px-[max(40px,calc((100%-880px)/2))] py-[42px]">
      {!chat || chat.messages.length === 0 ? <Welcome project={project} chat={chat} /> : (
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
  );
}
