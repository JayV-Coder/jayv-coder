import type { Chat } from "@/modules/core";
import { useChatRefusals } from "@/modules/connection";
import { useT } from "@/modules/i18n";
import { chatTitle } from "@/modules/workspace";
import { cn } from "@/lib/utils";
import { ConfirmAction } from "./ConfirmAction";

/** Uma linha de chat na lateral. O chat aberto tem barra acesa na borda, fundo
 * mais claro e título em destaque; o que o servidor recusou dele vira um "!". */
export function ChatRow({ chat, open, onOpen, onDelete }: { chat: Chat; open: boolean; onOpen: () => void; onDelete: () => void }) {
  const t = useT();
  const refused = useChatRefusals(chat.id);
  return (
    <div className="group flex min-w-0 items-center rounded-[7px]">
      <button
        type="button"
        aria-current={open ? "true" : undefined}
        title={open ? `${chatTitle(chat)} · ${t("chat.openNow")}` : chatTitle(chat)}
        onClick={onOpen}
        className={cn(
          "flex min-w-0 flex-1 items-center gap-2 rounded-[7px] py-2 pe-1.5 ps-[15px] text-start text-xs text-[#737d75] hover:bg-[#171c18] hover:text-[#dde5df]",
          open && "border-s-[3px] border-primary bg-[#1c241e] ps-3 text-[#f1f6f2]",
        )}
      >
        <span className={cn("w-2.5 flex-none text-center leading-none", open && "text-[9px] text-primary")}>{open ? "●" : "◌"}</span>
        <span className={cn("flex-1 truncate", open && "font-semibold")}>{chatTitle(chat)}</span>
        {refused > 0 && (
          <span role="status" title={t("chat.refused", { count: refused })} aria-label={t("chat.refused", { count: refused })} className="flex-none text-[11px] font-bold text-[#c9a86a]">!</span>
        )}
      </button>
      <ConfirmAction title={t("chat.delete.title")} description={t("chat.delete.description", { title: chatTitle(chat) })} onConfirm={onDelete}>
        <button
          type="button"
          title={t("chat.delete.title")}
          aria-label={t("chat.delete.aria", { title: chatTitle(chat) })}
          className="flex-[0_0_27px] p-1.5 text-base leading-none text-[#849087] opacity-0 group-hover:opacity-100 hover:text-destructive focus-visible:opacity-100"
        >
          ×
        </button>
      </ConfirmAction>
    </div>
  );
}
