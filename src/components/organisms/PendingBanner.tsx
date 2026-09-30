import type { Chat } from "@/modules/core";
import { liveOf, pendingWord, useConversation } from "@/modules/conversation";
import { openTurns } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { PulseDot } from "@/components/atoms";

/** A espera onde os olhos já estão: colada na caixa de escrita, e não no fim de
 * uma conversa que pode estar rolada para cima. Ela conta de quem é a vez —
 * `JayV` e o estado —, e embaixo o que está sendo feito agora.
 * Quem está na fila não ganha faixa própria: vira a contagem do cabeçalho, para
 * que a caixa não cresça a cada pedido empilhado. */
export function PendingBanner({ chat }: { chat: Chat | null }) {
  const t = useT();
  const live = useConversation((state) => state.live);
  const open = openTurns(chat);
  const turn = open[0];
  if (!turn) return null;
  const { text, beats } = liveOf(turn, live);
  const queued = open.length - 1;
  return (
    <div className="animate-pending-in border-b border-[#252d27] px-[18px] pt-[11px] pb-2.5 motion-reduce:animate-none">
      <div className="flex items-baseline gap-[9px]">
        <PulseDot />
        <strong className="text-xs font-bold text-[#d9e4db]">JayV</strong>
        <span className="ms-auto text-[11px] text-[#6e7870]">{queued > 0 ? t("pending.queued", { count: queued }) : t("pending.running")}</span>
      </div>
      <p className="shimmer-text mt-[5px] text-[12.5px] leading-[1.45]">{pendingWord(text, beats)}</p>
    </div>
  );
}
