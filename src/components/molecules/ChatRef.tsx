import type { Chat } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { chatTitle } from "@/modules/workspace";
import { ChatRefIcon } from "@/components/atoms";

/** O caminho de volta: cada cartão da portaria cita a conversa que o originou
 * e leva até ela. */
export function ChatRef({ chat, onOpen }: { chat: Chat | undefined; onOpen: () => void }) {
  const t = useT();
  return (
    <button
      type="button"
      disabled={!chat}
      title={chat ? t("gate.ref.open", { title: chatTitle(chat) }) : undefined}
      onClick={onOpen}
      className="flex max-w-full items-center gap-2 border border-rail-2 bg-[#0e1216] py-1.5 pe-2.5 ps-2 text-[12.5px] text-dim hover:enabled:border-[#48525c] hover:enabled:bg-[#141a20] hover:enabled:text-[#e3e8ed] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#7d8b99] disabled:cursor-default disabled:opacity-50"
    >
      <ChatRefIcon className="size-[15px] flex-none text-faint" />
      <span className="truncate">{chat ? chatTitle(chat) : t("gate.ref.gone")}</span>
    </button>
  );
}
