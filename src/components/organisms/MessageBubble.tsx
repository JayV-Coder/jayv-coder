import { useState, type ReactNode } from "react";
import { CheckIcon, CopyIcon, RotateCcwIcon } from "lucide-react";
import type { TurnView } from "@/modules/core";
import { messageLight, routeHint, routeLabel, shownText } from "@/modules/conversation";
import { formatClock, useT } from "@/modules/i18n";
import { Markdown } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** Onde uma mensagem do agente termina e a próxima começa (o
 * `MESSAGE_BREAK` do núcleo). */
const MESSAGE_BREAK = "⁣";

function splitMessages(text: string): string[] {
  const parts = text.split(MESSAGE_BREAK).map((part) => part.trim()).filter(Boolean);
  return parts.length > 0 ? parts : [text];
}

/** A cor que a portaria deu, escrita como no terminal: a luz e o veredito,
 * na cor da vez. */
function Verdict({ light }: { light: NonNullable<ReturnType<typeof messageLight>> }) {
  const t = useT();
  return (
    <span data-aspect={light.aspect} className="inline-flex items-center gap-1.5 text-[var(--aspect)]">
      <span aria-hidden="true" className="size-1.5 rounded-full bg-[var(--aspect)]" />
      {t(light.label)}
    </span>
  );
}

/** Uma mensagem dentro do bloco do turno, como um comando e a sua saída no
 * terminal. O pedido é a linha do prompt (`❯`), com a hora em cima; a resposta
 * vem logo abaixo, recuada, com quem a escreveu e o veredito da portaria. A
 * cor que a portaria deu pinta a margem do bloco (no `Timeline`) e, quando não
 * é verde, o veredito aparece escrito. */
export function MessageBubble({ role, content, turn, at, meta, pending, onRetry, onOpenFile, children }: {
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
  /** Abre um arquivo que a resposta citou. */
  onOpenFile?: (path: string) => void;
  children?: ReactNode;
}) {
  const t = useT();
  const [copied, setCopied] = useState(false);
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
  const tinted = light !== null && light.aspect !== "go";
  const animate = pending && "animate-pending-in motion-reduce:animate-none";

  const copy = async () => {
    await navigator.clipboard.writeText(answer);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  if (user) {
    return (
      <article aria-label={t("chat.you")} className={cn("grid gap-1", animate)}>
        <div className="flex min-h-5 flex-wrap items-center gap-x-3 gap-y-1 text-caption text-muted-foreground">
          <span>{t("chat.you")}</span>
          {at && <time dateTime={at}>{formatClock(at)}</time>}
          {/* O pedido que passou limpo não precisa de veredito: só o que a
              portaria barrou ou perguntou chama a atenção. */}
          {tinted && <span className="ms-auto"><Verdict light={light} /></span>}
        </div>
        <div data-aspect={light?.aspect} className="flex gap-2.5 font-medium">
          <span aria-hidden="true" className={cn("shrink-0 font-semibold text-go", tinted && "text-[var(--aspect)]")}>{tinted && light.aspect === "ask" ? "?" : "❯"}</span>
          <div className="min-w-0 leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere]">{text}</div>
        </div>
        {children}
        {turn?.status === "failed" && onRetry && (
          <div className="flex items-center gap-2.5 ps-[calc(1ch+0.625rem)]">
            <Button variant="outline" size="xs" title={t("chat.retry.title")} onClick={onRetry}>
              <RotateCcwIcon aria-hidden="true" />
              {t("chat.retry")}
            </Button>
            <small className="text-caption text-muted-foreground">{t("chat.retry.note")}</small>
          </div>
        )}
      </article>
    );
  }

  return (
    <article aria-label="JayV" className={cn("group/message grid gap-1.5 ps-[calc(1ch+0.625rem)]", animate)}>
      <div className="flex min-h-5 flex-wrap items-center gap-x-3 gap-y-1 text-caption text-muted-foreground">
        <span className="text-foreground">jayv</span>
        {route && (
          <small data-mode={route.mode ?? undefined} title={routeHint(route)} className="min-w-0 truncate text-caption data-[mode=build]:text-success">
            {routeLabel(route)}
          </small>
        )}
        {light && <span className="ms-auto"><Verdict light={light} /></span>}
      </div>
      {(text || !pending) && (
        <div className="grid gap-2">
          {parts.map((part, index) => {
            const last = index === parts.length - 1;
            return (
              <div
                key={index}
                data-aspect={light?.aspect}
                className={cn(
                  "leading-relaxed [overflow-wrap:anywhere]",
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
      {!pending && (meta || answer) && (
        <div className="flex min-h-6 items-center gap-2 text-caption text-muted-foreground">
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
          {meta && <small className="tabular-nums">{meta}</small>}
        </div>
      )}
      {pending && meta && <small className="block text-caption text-muted-foreground">{meta}</small>}
    </article>
  );
}
