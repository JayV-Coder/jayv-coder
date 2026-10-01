import type { Chat } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { cn } from "@/lib/utils";
import { ConfirmAction } from "./ConfirmAction";

/** Uma linha de chat na lateral. O chat aberto tem barra acesa na borda, fundo
 * mais claro e título em destaque. */
export function ChatRow({ chat, open, onOpen, onDelete }: { chat: Chat; open: boolean; onOpen: () => void; onDelete: () => void }) {
  const t = useT();
  return (
    <div className="group flex min-w-0 items-center rounded-[7px]">
      <button
        type="button"
        aria-current={open ? "true" : undefined}
        title={open ? `${chat.title} · ${t("chat.openNow")}` : chat.title}
        onClick={onOpen}
        className={cn(
          "flex min-w-0 flex-1 items-center gap-2 rounded-[7px] py-2 pe-1.5 ps-[15px] text-start text-xs text-[#737d75] hover:bg-[#171c18] hover:text-[#dde5df]",
          open && "border-s-[3px] border-primary bg-[#1c241e] ps-3 text-[#f1f6f2]",
        )}
      >
        <span className={cn("w-2.5 flex-none text-center leading-none", open && "text-[9px] text-primary")}>{open ? "●" : "◌"}</span>
        <span className={cn("flex-1 truncate", open && "font-semibold")}>{chat.title}</span>
      </button>
      <ConfirmAction title={t("chat.delete.title")} description={t("chat.delete.description", { title: chat.title })} onConfirm={onDelete}>
        <button
          type="button"
          title={t("chat.delete.title")}
          aria-label={t("chat.delete.aria", { title: chat.title })}
          className="flex-[0_0_27px] p-1.5 text-base leading-none text-[#849087] opacity-0 group-hover:opacity-100 hover:text-destructive focus-visible:opacity-100"
        >
          ×
        </button>
      </ConfirmAction>
    </div>
  );
}
