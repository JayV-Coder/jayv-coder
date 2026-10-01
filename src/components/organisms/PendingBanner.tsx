import { useEffect, useRef } from "react";
import type { Chat } from "@/modules/core";
import { beatLines, liveOf, pendingWord, useConversation } from "@/modules/conversation";
import { openTurns } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { PulseDot } from "@/components/atoms";

/** A espera onde os olhos já estão: colada na caixa de escrita, e não no fim de
 * uma conversa que pode estar rolada para cima. Ela conta de quem é a vez —
 * `JayV` e o estado —, embaixo cada etapa do Jev e da portaria que já passou e,
 * por último, o que está sendo feito agora.
 * Quem está na fila não ganha faixa própria: vira a contagem do cabeçalho, para
 * que a caixa não cresça a cada pedido empilhado. */
export function PendingBanner({ chat }: { chat: Chat | null }) {
  const t = useT();
  const steps = useRef<HTMLOListElement>(null);
  const live = useConversation((state) => state.live);
  const open = openTurns(chat);
  const turn = open[0];
  const { text, beats } = turn ? liveOf(turn, live) : { text: "", beats: [] };
  const lines = beatLines(beats);
  // A última etapa é a de agora e vai na linha de baixo; as anteriores ficam
  // na lista, que rola sozinha para a mais recente.
  const done = text ? lines : lines.slice(0, -1);

  useEffect(() => {
    const element = steps.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [done.length]);

  if (!turn) return null;
  const queued = open.length - 1;
  return (
    <div className="animate-pending-in border-b border-[#252d27] px-[18px] pt-[11px] pb-2.5 motion-reduce:animate-none" role="status" aria-live="polite">
      <div className="flex items-baseline gap-[9px]">
        <PulseDot />
        <strong className="text-xs font-bold text-[#d9e4db]">JayV</strong>
        <span className="ms-auto text-[11px] text-[#6e7870]">{queued > 0 ? t("pending.queued", { count: queued }) : t("pending.running")}</span>
      </div>
      {done.length > 0 && (
        <ol ref={steps} className="mt-[7px] grid max-h-[6.75rem] gap-1 overflow-y-auto font-mono text-[11.5px] leading-[1.45] text-[#7f8981]">
          {done.map((line) => <li key={line.seq} data-kind={line.kind} className="data-[kind=failed]:text-destructive">{line.line}</li>)}
        </ol>
      )}
      <p className="shimmer-text mt-[5px] text-[12.5px] leading-[1.45]">{pendingWord(text, beats)}</p>
    </div>
  );
}
