import { useEffect, useState, type ReactNode } from "react";
import { CheckIcon, CopyIcon, ListChecksIcon, RotateCcwIcon, Undo2Icon, XIcon } from "lucide-react";
import type { TurnView } from "@/modules/core";
import { answerLines, messageLight, routeHint, routeLabel, shownText, type MessageLight } from "@/modules/conversation";
import { ChevronIcon } from "@/components/atoms";
import { formatClock, useT } from "@/modules/i18n";
import { Markdown } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { EvidencePanel } from "./EvidencePanel";

/** Onde uma mensagem do agente termina e a próxima começa (o
 * `MESSAGE_BREAK` do núcleo). */
const MESSAGE_BREAK = "⁣";

function splitMessages(text: string): string[] {
  const parts = text.split(MESSAGE_BREAK).map((part) => part.trim()).filter(Boolean);
  return parts.length > 0 ? parts : [text];
}

/** A cor que a portaria deu, escrita como no terminal: a luz e o veredito,
 * na cor da vez. A resposta não verificada não tem cor: o ponto fica vazado,
 * e passar o mouse diz por quê. */
function Verdict({ light }: { light: MessageLight }) {
  const t = useT();
  if (light.aspect === null) {
    return (
      <span title={t("verdict.unverified.hint")} className="inline-flex items-center gap-1.5 text-muted-foreground">
        <span aria-hidden="true" className="size-1.5 rounded-full border border-current" />
        {t(light.label)}
      </span>
    );
  }
  return (
    <span data-aspect={light.aspect} className="inline-flex items-center gap-1.5 text-[var(--aspect)]">
      <span aria-hidden="true" className="size-1.5 rounded-full bg-[var(--aspect)]" />
      {t(light.label)}
    </span>
  );
}

/** Uma mensagem dentro do bloco do turno, como um comando e a sua saída no
 * terminal. Cada lado tem o seu balão, para não haver dúvida de quem falou:
 * o pedido é a linha do prompt (`❯`) num balão cheio, à direita, com a hora
 * em cima; a resposta é uma janela de terminal — barra com 🤖 jayv, a rota e
 * o veredito, e a saída embaixo. A cor que a portaria deu pinta a margem do
 * bloco (no `Timeline`) e, quando não é verde, a borda do pedido e o veredito
 * escrito. */
export function MessageBubble({ role, content, turn, at, meta, pending, onRetry, onCancel, onOpenFile, onUndoMode, children }: {
  role: "user" | "assistant";
  content: string;
  turn: TurnView | null;
  /** Quando a mensagem foi gravada. */
  at?: string;
  meta?: string;
  pending?: boolean;
  /** O pedido que não chegou ao fim continua escrito, e ganha a chance de ir
   * de novo — como o mesmo pedido, para não virar dois no histórico.
   * Quem foi barrado não recebe este botão: a portaria recusou de propósito. */
  onRetry?: () => void;
  /** Tira da fila o pedido que ainda espera a vez. */
  onCancel?: () => void;
  /** Abre um arquivo que a resposta citou. */
  onOpenFile?: (path: string) => void;
  /** Devolve o chat ao modo de onde o Jev o tirou neste pedido. */
  onUndoMode?: () => void;
  children?: ReactNode;
}) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const [thoughts, setThoughts] = useState(false);
  // Respostas a várias perguntas do agente ocupam o chat: ficam recolhidas
  // numa linha, e abrem com um clique.
  const answers = role === "user" ? answerLines(content) : null;
  const foldable = answers !== null && answers.length > 1;
  const [unfolded, setUnfolded] = useState(false);
  const [checked, setChecked] = useState(false);
  const light = turn ? messageLight(role, turn) : null;
  const user = role === "user";
  // Pedido barrado, falha e resposta a uma pergunta ficam gravados como aviso
  // e são ditos aqui no idioma de quem lê.
  const text = shownText(content);
  // A resposta diz quem a escreveu: o agente, o modelo, o modo e o papel que
  // o Jev escolheu para o pedido.
  const route = !user && turn?.route ? turn.route : null;
  // Cada mensagem do agente no seu trecho, como o Claude e o Codex mostram:
  // o que ele avisou no caminho não se mistura com a resposta final.
  const parts = user ? [text] : splitMessages(text);
  const answer = parts[parts.length - 1] ?? "";
  // O que o agente pensou em voz alta no caminho fica recolhido: só o mais
  // recente aparece, e um botão abre os anteriores.
  const said = parts.slice(0, -1);
  const earlier = thoughts ? 0 : Math.max(0, said.length - 1);
  const shown = [...said.slice(earlier), answer];
  // Pedido novo, balão recolhido de novo.
  useEffect(() => { setThoughts(false); setUnfolded(false); setChecked(false); }, [turn?.id]);
  const tinted = light !== null && light.aspect !== null && light.aspect !== "go";
  // O que foi conferido só existe para a resposta que chegou ao fim.
  const checkable = !user && !pending && turn?.status === "answered";
  const animate = pending && "animate-pending-in motion-reduce:animate-none";

  const copy = async () => {
    await navigator.clipboard.writeText(answer);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  if (user) {
    return (
      <article aria-label={t("chat.you")} className={cn("grid justify-items-end gap-1.5", animate)}>
        <div
          data-aspect={light?.aspect}
          data-tinted={tinted || undefined}
          className="grid w-fit max-w-[85%] min-w-[min(18rem,85%)] gap-1 rounded-md border border-border bg-secondary/60 px-3.5 pt-2 pb-2.5 data-[tinted]:border-[color-mix(in_srgb,var(--aspect)_55%,var(--border))]"
        >
          <div className="flex min-h-5 flex-wrap items-center gap-x-3 gap-y-1 text-caption text-dim">
            <span className="font-semibold text-foreground">{t("chat.you")}</span>
            {at && <time dateTime={at}>{formatClock(at)}</time>}
            {/* O pedido que passou limpo não precisa de veredito: só o que a
                portaria barrou ou perguntou chama a atenção. */}
            {tinted && <span className="ms-auto"><Verdict light={light} /></span>}
          </div>
          <div className="flex gap-2.5 font-medium">
            <span aria-hidden="true" className={cn("shrink-0 font-semibold text-go", tinted && "text-[var(--aspect)]")}>{tinted && light.aspect === "ask" ? "?" : "❯"}</span>
            {foldable ? (
              <div className="grid min-w-0 gap-1">
                <button
                  type="button"
                  aria-expanded={unfolded}
                  onClick={() => setUnfolded(!unfolded)}
                  className="-ms-1 flex items-center gap-1.5 rounded-sm px-1 text-start underline-offset-2 hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                >
                  <ChevronIcon className={cn("w-[15px] flex-none transition-transform duration-200 motion-reduce:transition-none", !unfolded && "-rotate-90")} />
                  {t("chat.answers", { count: answers.length })}
                </button>
                {unfolded && <div className="min-w-0 leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere]">{text}</div>}
              </div>
            ) : (
              <div className="min-w-0 leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere]">{text}</div>
            )}
          </div>
        </div>
        {children}
        {turn?.status === "queued" && onCancel && (
          <Button variant="ghost" size="xs" className="text-muted-foreground" title={t("chat.cancelQueued.title")} onClick={onCancel}>
            <XIcon aria-hidden="true" />
            {t("chat.cancelQueued")}
          </Button>
        )}
        {turn?.status === "failed" && onRetry && (
          <div className="flex items-center gap-2.5">
            <small className="text-caption text-muted-foreground">{t("chat.retry.note")}</small>
            <Button variant="outline" size="xs" title={t("chat.retry.title")} onClick={onRetry}>
              <RotateCcwIcon aria-hidden="true" />
              {t("chat.retry")}
            </Button>
          </div>
        )}
      </article>
    );
  }

  return (
    <article aria-label="JayV" className={cn("group/message min-w-0 overflow-hidden rounded-md border border-border bg-card", animate)}>
      <div className="flex min-h-7 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border bg-secondary/50 px-3.5 py-1 text-caption text-dim">
        <span className="inline-flex items-center gap-1.5 font-semibold text-foreground">
          <span aria-hidden="true" className="text-[0.8125rem] leading-none">🤖</span>
          jayv
        </span>
        {route && (
          <small data-mode={route.mode ?? undefined} title={routeHint(route)} className="min-w-0 truncate text-caption data-[mode=build]:text-success">
            {routeLabel(route)}
          </small>
        )}
        {light && <span className="ms-auto"><Verdict light={light} /></span>}
      </div>
      <div className="grid grid-cols-[minmax(0,1fr)] gap-1.5 px-3.5 pt-2.5 pb-2">
        {route?.switched && (
          // O Jev tirou o chat do planejamento: uma linha diz por quê e, na
          // troca mais recente, deixa voltar.
          <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-caption text-muted-foreground">
            <span className="before:me-1.5 before:text-success before:content-['↻']">{t(route.switched.reason === "repeated" ? "mode.switched.repeated" : "mode.switched.asked")}</span>
            {onUndoMode && (
              <Button variant="outline" size="xs" title={t("mode.undo.hint", { name: t(`mode.${route.switched.from}`) })} onClick={onUndoMode}>
                <Undo2Icon aria-hidden="true" />
                {t("mode.undo")}
              </Button>
            )}
          </div>
        )}
        {(text || !pending) && (
          <div className="grid grid-cols-[minmax(0,1fr)] gap-2">
            {said.length > 1 && (
              <button
                type="button"
                aria-expanded={thoughts}
                onClick={() => setThoughts(!thoughts)}
                className="-ms-1 w-fit rounded-xs px-1 text-start text-caption text-muted-foreground underline-offset-[3px] hover:text-foreground hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              >
                {thoughts ? t("chat.thoughts.less") : t("chat.thoughts.more", { count: earlier })}
              </button>
            )}
            {shown.map((part, index) => {
              const last = index === shown.length - 1;
              return (
                <div
                  key={earlier + index}
                  data-aspect={light?.aspect}
                  className={cn(
                    "min-w-0 leading-relaxed [overflow-wrap:anywhere]",
                    // O que o agente disse no caminho: discreto, como a saída
                    // de uma ferramenta.
                    !last && "flex gap-2 text-small text-muted-foreground before:shrink-0 before:text-faint before:content-['↳']",
                    last && tinted && "text-[var(--aspect)]",
                    pending && "whitespace-pre-wrap",
                  )}
                >
                  {pending ? part : <Markdown content={part} onOpenFile={onOpenFile} />}
                </div>
              );
            })}
          </div>
        )}
        {children}
        {checkable && checked && <EvidencePanel turnId={turn.id} />}
        {!pending && (meta || answer || checkable) && (
          <div className="flex min-h-6 items-center gap-2 text-caption text-muted-foreground">
            {checkable && (
              <Button
                variant="ghost"
                size="xs"
                aria-expanded={checked}
                onClick={() => setChecked(!checked)}
                className="h-5 text-caption font-normal text-muted-foreground"
              >
                <ListChecksIcon aria-hidden="true" />
                {t("evidence.toggle")}
              </Button>
            )}
            {answer && (
              <Button
                variant="outline"
                size="xs"
                onClick={() => void copy()}
                title={t(copied ? "chat.copied" : "chat.copy")}
                className="h-5 text-caption font-normal text-muted-foreground opacity-0 transition-opacity group-hover/message:opacity-100 focus-visible:opacity-100 data-[copied=true]:opacity-100"
                data-copied={copied}
              >
                {copied ? <CheckIcon aria-hidden="true" /> : <CopyIcon aria-hidden="true" />}
                {t(copied ? "chat.copied" : "chat.copy")}
              </Button>
            )}
            {meta && <small className="ms-auto text-end tabular-nums">{meta}</small>}
          </div>
        )}
        {pending && meta && <small className="block text-end text-caption text-muted-foreground">{meta}</small>}
      </div>
    </article>
  );
}
