import { useEffect, useRef } from "react";
import type { Chat, Project } from "@/modules/core";
import { sendPrompt, useConversation } from "@/modules/conversation";
import { openTurns } from "@/modules/workspace";
import { MessageBubble } from "./MessageBubble";
import { PendingBubble } from "./PendingBubble";
import { Welcome } from "./Welcome";

/** A conversa inteira: o que está gravado e, no fim, os pedidos em aberto. */
export function Timeline({ chat, project }: { chat: Chat | null; project: Project | null }) {
  const scroller = useRef<HTMLDivElement>(null);
  const live = useConversation((state) => state.live);
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
                onRetry={turn ? () => retry(turn.id) : undefined}
              />
            );
          })}
          {open.map((turn, index) => <PendingBubble key={turn.id} turn={turn} place={index + 1} />)}
        </>
      )}
    </div>
  );
}
