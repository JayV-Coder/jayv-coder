import { useEffect, useRef, useState } from "react";
import { ArrowDownIcon } from "lucide-react";
import type { Aspect, Chat, Message, Project, TurnView } from "@/modules/core";
import { cancelTurn, messageLight, sendPrompt, useConversation } from "@/modules/conversation";
import { useLocale, useT } from "@/modules/i18n";
import { formatCost, formatDuration, formatTokens, useUsage } from "@/modules/usage";
import { openFile, openTurns, setWorkMode } from "@/modules/workspace";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { ChatFind } from "./ChatFind";
import { MessageBubble } from "./MessageBubble";
import { PendingBubble } from "./PendingBubble";
import { Welcome } from "./Welcome";

/** Quanto do fim ainda conta como "no fim": a folga de uma linha de texto. */
const NEAR_BOTTOM = 96;

const WEIGHT: Record<Aspect, number> = { go: 0, ask: 1, stop: 2 };

interface Block {
  key: string;
  turn: TurnView | null;
  messages: { message: Message; index: number }[];
}

/** Junta pedido e resposta de um mesmo turno num bloco, como um comando e a
 * saída dele no terminal. Mensagem sem turno fica num bloco só seu. */
export function blocksOf(messages: Message[], turns: Map<string, TurnView>): Block[] {
  const blocks: Block[] = [];
  messages.forEach((message, index) => {
    const last = blocks[blocks.length - 1];
    if (message.turnId && last?.turn?.id === message.turnId) {
      last.messages.push({ message, index });
      return;
    }
    const turn = message.turnId ? turns.get(message.turnId) ?? null : null;
    blocks.push({ key: `${message.turnId ?? "m"}-${index}`, turn, messages: [{ message, index }] });
  });
  return blocks;
}

/** A cor da margem do bloco: a pior luz que a portaria deu ao pedido ou à
 * resposta. Verde não pinta nada, para o âmbar e o vermelho chamarem a
 * atenção — como o Warp pinta só o comando que falhou. */
export function blockAspect(turn: TurnView | null): Aspect | null {
  if (!turn) return null;
  const aspects = [messageLight("user", turn)?.aspect, messageLight("assistant", turn)?.aspect].filter((aspect) => aspect != null);
  const worst = aspects.reduce<Aspect>((found, aspect) => (WEIGHT[aspect] > WEIGHT[found] ? aspect : found), "go");
  return worst === "go" ? null : worst;
}

/** A conversa inteira, em blocos de pedido e resposta: o que está gravado e,
 * dentro do bloco de cada pedido em aberto, a resposta enquanto chega.
 *
 * A rolagem segue a resposta só enquanto quem lê está no fim. Quem subiu para
 * reler um trecho fica onde está, e um botão leva de volta ao fim. */
export function Timeline({ chat, project }: { chat: Chat | null; project: Project | null }) {
  const scroller = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const [away, setAway] = useState(false);
  const t = useT();
  const locale = useLocale();
  const spent = useUsage((state) => (chat ? state.turns[chat.id] : undefined));
  // O rodapé da resposta: os tokens e o tempo do turno, com `≈` quando a
  // ferramenta não os informou.
  const meta = (turnId: string) => {
    const usage = spent?.[turnId];
    if (!usage) return undefined;
    const parts = [t("usage.turn.tokens", { input: formatTokens(usage.inputTokens, locale), output: formatTokens(usage.outputTokens, locale) })];
    if (usage.costUsd !== null) parts.push(formatCost(usage.costUsd, locale));
    if (usage.durationMs > 0) parts.push(formatDuration(usage.durationMs, locale));
    return `${usage.estimated ? "≈ " : ""}${parts.join(" · ")}`;
  };
  const turns = new Map((chat?.turns ?? []).map((turn) => [turn.id, turn]));
  const open = openTurns(chat);
  const sent = chat?.messages.filter((message) => message.role === "user").length ?? 0;
  const blocks = blocksOf(chat?.messages ?? [], turns);
  const grouped = new Set(blocks.map((block) => block.turn?.id).filter(Boolean));
  // Só a troca de modo mais recente do Jev pode ser desfeita, e só enquanto o
  // chat continua no modo em que ela o deixou.
  const lastSwitch = [...(chat?.turns ?? [])].reverse().find((turn) => turn.route?.switched);
  const undoable = chat?.workMode === "build" ? lastSwitch?.id : undefined;

  const toBottom = (smooth = false) => {
    const element = scroller.current;
    if (!element) return;
    element.scrollTo({ top: element.scrollHeight, behavior: smooth ? "smooth" : "auto" });
    pinned.current = true;
    setAway(false);
  };

  // Abrir outro chat ou mandar um pedido novo volta ao fim: é lá que a
  // resposta vai aparecer.
  useEffect(() => toBottom(), [chat?.id, sent, open.length]);

  useEffect(() => {
    if (pinned.current) toBottom();
  }, [chat]);

  // O texto que chega rola a conversa sem redesenhá-la: a linha do tempo
  // inteira não renasce a cada quadro, só o balão em aberto.
  useEffect(() => useConversation.subscribe((state, before) => {
    if (state.live !== before.live && pinned.current) toBottom();
  }), []);

  const onScroll = () => {
    const element = scroller.current;
    if (!element) return;
    const near = element.scrollHeight - element.scrollTop - element.clientHeight < NEAR_BOTTOM;
    pinned.current = near;
    setAway(!near);
  };

  const retry = (turnId: string) => {
    const said = chat?.messages.find((message) => message.turnId === turnId && message.role === "user");
    if (chat && said) void sendPrompt(said.content, chat.id, turnId);
  };

  // O chat ainda sem a conversa lida não é um chat vazio: as boas-vindas não
  // piscam enquanto ela chega.
  const empty = !chat || Math.max(chat.messages.length, chat.messageCount ?? 0) === 0;
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <ChatFind root={scroller} version={chat} />
      <div ref={scroller} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto">
        <div className={cn("mx-auto min-h-full w-full max-w-4xl px-5 pt-2 pb-4", empty && "flex flex-col pt-6")}>
          {empty ? <Welcome project={project} chat={chat} /> : (
            <>
              {blocks.map((block) => {
                const { turn } = block;
                const place = turn ? open.findIndex((item) => item.id === turn.id) : -1;
                return (
                  <section
                    key={block.key}
                    data-aspect={blockAspect(turn) ?? undefined}
                    className="relative grid grid-cols-[minmax(0,1fr)] gap-3 border-b border-border py-3.5 ps-5 pe-1 before:absolute before:inset-y-0 before:start-0 before:w-[3px] data-[aspect]:before:bg-[var(--aspect)] data-[aspect=stop]:bg-[color-mix(in_srgb,var(--stop)_5%,transparent)]"
                  >
                    {block.messages.map(({ message, index }) => (
                      <MessageBubble
                        key={`${message.role}-${index}`}
                        role={message.role}
                        content={message.content}
                        at={message.createdAt}
                        turn={turn}
                        meta={message.role === "assistant" && message.turnId ? meta(message.turnId) : undefined}
                        onRetry={turn ? () => retry(turn.id) : undefined}
                        onCancel={chat && turn ? () => void cancelTurn(turn.id, chat.id) : undefined}
                        onUndoMode={chat && turn && turn.id === undoable && turn.route?.switched ? () => void setWorkMode(chat.id, turn.route?.switched?.from ?? "auto") : undefined}
                        onOpenFile={project?.rootPath ? (path) => void openFile(chat.id, path) : undefined}
                      />
                    ))}
                    {turn && place >= 0 && <PendingBubble turn={turn} place={place + 1} />}
                  </section>
                );
              })}
              {open.filter((turn) => !grouped.has(turn.id)).map((turn) => (
                <section key={turn.id} className="grid grid-cols-[minmax(0,1fr)] py-3.5 ps-5"><PendingBubble turn={turn} place={open.indexOf(turn) + 1} /></section>
              ))}
            </>
          )}
        </div>
      </div>
      {away && !empty && (
        <Button
          variant="outline"
          size="sm"
          onClick={() => toBottom(true)}
          className="absolute bottom-3 left-1/2 -translate-x-1/2 bg-card animate-pending-in motion-reduce:animate-none"
        >
          <ArrowDownIcon aria-hidden="true" />
          {t("chat.jumpToLatest")}
        </Button>
      )}
    </div>
  );
}
