import { Trash2Icon } from "lucide-react";
import { shorten, type Chat } from "@/modules/core";
import { shownText } from "@/modules/conversation";
import { lastGatePass, useGate } from "@/modules/gate";
import { formatSince, useT } from "@/modules/i18n";
import { chatTitle } from "@/modules/workspace";
import { ConfirmAction, GatePass } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { YardCard } from "./YardCard";

/** O cartão de um chat: como ele se chama, quando nasceu, o que a portaria
 * registrou nele por último e o pedido mais recente que ele recebeu. */
export function ChatCard({ chat, onOpen, onDelete }: { chat: Chat; onOpen: () => void; onDelete: () => void }) {
  const t = useT();
  const feed = useGate((state) => state.feed);
  const last = [...chat.messages].reverse().find((message) => message.role === "user")?.content;
  const said = last === undefined ? undefined : shownText(last);
  return (
    <YardCard
      onOpen={onOpen}
      actions={(
        <ConfirmAction title={t("chat.delete.title")} description={t("chat.delete.description", { title: chat.title })} onConfirm={onDelete}>
          <Button variant="ghost" size="sm" className="ms-auto text-muted-foreground hover:text-destructive">
            <Trash2Icon aria-hidden="true" />
            {t("common.delete")}
          </Button>
        </ConfirmAction>
      )}
    >
      <span className="text-lg font-semibold break-words">{chatTitle(chat)}</span>
      <span className="text-xs text-muted-foreground">{t("chat.createdAt", { date: formatSince(chat.createdAt) })}</span>
      <GatePass pass={lastGatePass(feed, chat.id)} />
      <span className={cn("text-sm leading-relaxed text-muted-foreground", !said && "text-muted-foreground italic")}>
        {said ? shorten(said, 150) : t("chat.noPrompt")}
      </span>
    </YardCard>
  );
}
