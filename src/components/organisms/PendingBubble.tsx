import type { TurnView } from "@/modules/core";
import { beatLines, liveOf, useConversation } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { MessageBubble } from "./MessageBubble";

/** O que o desenvolvedor lê enquanto espera. O primeiro da fila está sendo
 * atendido; os outros dizem quantos estão na frente, para que ficar em fila
 * seja uma informação e não um silêncio. Quando já há texto chegando, é o texto
 * que aparece — em texto puro, porque marcação pela metade pisca na tela; a
 * resposta formatada vem no redesenho, quando o pedido fecha.
 *
 * O balão só aparece quando já tem o que mostrar. Enquanto o pedido não deu
 * notícia nenhuma, quem conta a espera é a faixa da caixa de escrita. */
export function PendingBubble({ turn, place }: { turn: TurnView; place: number }) {
  const t = useT();
  const live = useConversation((state) => state.live);
  const { text, beats } = liveOf(turn, live);
  const lines = beatLines(beats);
  if (!text && beats.length === 0) return null;
  return (
    <MessageBubble role="assistant" content={text} turn={turn} meta={place > 1 ? t("pending.ahead", { count: place - 1 }) : ""} pending>
      {lines.length > 0 && (
        <ol className="mt-2.5 grid gap-1 px-1 font-mono text-[11.5px] text-[#7f8981]">
          {lines.map((line) => <li key={line.seq} data-kind={line.kind} className="data-[kind=failed]:text-destructive">{line.line}</li>)}
        </ol>
      )}
    </MessageBubble>
  );
}
