import { useEffect, useRef, useState } from "react";
import type { Chat } from "@/modules/core";
import { beatLines, liveOf, pendingWord, useConversation } from "@/modules/conversation";
import { openTurns } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { PulseDot } from "@/components/atoms";

/** Quantas etapas já passadas ficam à vista antes de pedir para abrir. */
const RECENT = 3;

/** A espera onde os olhos já estão: colada na caixa de escrita, e não no fim de
 * uma conversa que pode estar rolada para cima. Ela conta de quem é a vez —
 * `JayV` e o estado —, embaixo cada etapa do Jev e da portaria que já passou e,
 * por último, o que está sendo feito agora.
 * Só as três etapas mais recentes aparecem; as anteriores ficam atrás de um
 * botão, para a faixa não empurrar a conversa para cima.
 * Quem está na fila não ganha faixa própria: vira a contagem do cabeçalho, para
 * que a caixa não cresça a cada pedido empilhado. */
export function PendingBanner({ chat }: { chat: Chat | null }) {
  const t = useT();
  const steps = useRef<HTMLOListElement>(null);
  const [all, setAll] = useState(false);
  const live = useConversation((state) => state.live);
  const open = openTurns(chat);
  const turn = open[0];
  const { text, beats } = turn ? liveOf(turn, live) : { text: "", beats: [] };
  const lines = beatLines(beats);
  // A última etapa é a de agora e vai na linha de baixo; as anteriores ficam
  // na lista, que rola sozinha para a mais recente.
  const done = text ? lines : lines.slice(0, -1);
  const hidden = Math.max(0, done.length - RECENT);
  const shown = all ? done : done.slice(hidden);

  // Pedido novo começa recolhido de novo.
  useEffect(() => setAll(false), [turn?.id]);

  useEffect(() => {
    const element = steps.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [done.length, all]);

  if (!turn) return null;
  const queued = open.length - 1;
  return (
    <div className="animate-pending-in border-b border-border px-[18px] pt-[11px] pb-2.5 motion-reduce:animate-none" role="status" aria-live="polite">
      <div className="flex items-baseline gap-[9px]">
        <PulseDot />
        <strong className="text-xs font-semibold text-foreground"><span aria-hidden="true" className="me-1.5">🤖</span>JayV</strong>
        <span className="ms-auto text-caption text-muted-foreground">{queued > 0 ? t("pending.queued", { count: queued }) : t("pending.running")}</span>
      </div>
      {hidden > 0 && (
        <button
          type="button"
          aria-expanded={all}
          onClick={() => setAll(!all)}
          className="mt-[7px] -ms-1 rounded-xs px-1 text-caption text-muted-foreground underline-offset-[3px] hover:text-foreground hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        >
          {all ? t("pending.steps.less") : t("pending.steps.all", { count: hidden })}
        </button>
      )}
      {shown.length > 0 && (
        <ol ref={steps} className="mt-[7px] grid max-h-[40vh] gap-1 overflow-y-auto font-mono text-xs leading-[1.45] text-muted-foreground">
          {shown.map((line) => <li key={line.seq} data-kind={line.kind} className="data-[kind=failed]:text-destructive">{line.line}</li>)}
        </ol>
      )}
      <p className="shimmer-text mt-[5px] text-xs leading-[1.45]">{pendingWord(text, beats)}</p>
    </div>
  );
}
