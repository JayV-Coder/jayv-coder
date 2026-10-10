import { XIcon } from "lucide-react";
import type { Chat } from "@/modules/core";
import { useChatRefusals } from "@/modules/connection";
import { useT } from "@/modules/i18n";
import { chatTitle } from "@/modules/workspace";
import { cn } from "@/lib/utils";
import { ConfirmAction } from "./ConfirmAction";

/** Uma linha de chat na lateral. O chat aberto tem traço na borda, fundo
 * destacado e título em destaque; o que o servidor recusou dele vira um "!".
 * `detail` é uma segunda linha apagada (quando foi a última conversa). */
export function ChatRow({ chat, open, onOpen, onDelete, detail }: { chat: Chat; open: boolean; onOpen: () => void; onDelete: () => void; detail?: string }) {
  const t = useT();
  const refused = useChatRefusals(chat.id);
  return (
    <div className="group flex min-w-0 items-center rounded-md">
      <button
        type="button"
        aria-current={open ? "true" : undefined}
        title={open ? `${chatTitle(chat)} · ${t("chat.openNow")}` : chatTitle(chat)}
        onClick={onOpen}
        className={cn(
          "relative flex min-w-0 flex-1 items-center gap-2 rounded-md py-1.5 pe-1.5 ps-3 text-start text-[13px] text-sidebar-muted transition-colors outline-none hover:bg-sidebar-accent/70 hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-ring",
          open && "bg-sidebar-accent font-medium text-sidebar-foreground before:absolute before:inset-y-1.5 before:start-0 before:w-0.5 before:rounded-full before:bg-foreground",
        )}
      >
        {detail
          ? (
            <span className="grid min-w-0 flex-1">
              <span className="truncate">{chatTitle(chat)}</span>
              <span className="truncate text-caption font-normal text-sidebar-muted">{detail}</span>
            </span>
          )
          : <span className="flex-1 truncate">{chatTitle(chat)}</span>}
        {refused > 0 && (
          <span role="status" title={t("chat.refused", { count: refused })} aria-label={t("chat.refused", { count: refused })} className="flex-none font-mono text-caption font-semibold text-warning">!</span>
        )}
      </button>
      <ConfirmAction title={t("chat.delete.title")} description={t("chat.delete.description", { title: chatTitle(chat) })} onConfirm={onDelete}>
        <button
          type="button"
          title={t("chat.delete.title")}
          aria-label={t("chat.delete.aria", { title: chatTitle(chat) })}
          className="grid size-7 flex-none place-items-center rounded-md text-sidebar-muted opacity-0 transition-opacity group-hover:opacity-100 hover:text-destructive focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        >
          <XIcon aria-hidden="true" className="size-4" />
        </button>
      </ConfirmAction>
    </div>
  );
}
