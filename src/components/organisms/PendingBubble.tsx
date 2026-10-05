import type { TurnView } from "@/modules/core";
import { liveOf, useConversation } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { MessageBubble } from "./MessageBubble";

/** A resposta enquanto chega. O primeiro da fila está sendo atendido; os
 * outros dizem quantos estão na frente. O texto aparece em texto puro, porque
 * marcação pela metade pisca na tela; a resposta formatada vem no redesenho,
 * quando o pedido fecha.
 *
 * O balão só aparece quando há texto. As etapas do Jev e da portaria ficam na
 * faixa colada à caixa de escrita (`PendingBanner`), e não soltas na conversa. */
export function PendingBubble({ turn, place }: { turn: TurnView; place: number }) {
  const t = useT();
  // Só o pedido deste balão: o pedaço de outro turno não o redesenha.
  const live = useConversation((state) => state.live[turn.id]);
  const { text } = liveOf(turn, live);
  if (!text) return null;
  return <MessageBubble role="assistant" content={text} turn={turn} meta={place > 1 ? t("pending.ahead", { count: place - 1 }) : ""} pending />;
}
