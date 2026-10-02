import { useState, type ReactNode } from "react";
import { CheckIcon, CopyIcon, RotateCcwIcon } from "lucide-react";
import type { TurnView } from "@/modules/core";
import { messageLight, routeHint, routeLabel, shownText } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { BrandMark } from "@/components/atoms";
import { Markdown } from "@/components/molecules";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** Onde uma mensagem do agente termina e a próxima começa (o
 * `MESSAGE_BREAK` do núcleo). */
const MESSAGE_BREAK = "⁣";

function splitMessages(text: string): string[] {
  const parts = text.split(MESSAGE_BREAK).map((part) => part.trim()).filter(Boolean);
  return parts.length > 0 ? parts : [text];
}

/** A cor que a portaria deu ao pedido, num selo com a luz na frente. */
function LightBadge({ light }: { light: NonNullable<ReturnType<typeof messageLight>> }) {
  const t = useT();
  return (
    <Badge
      variant="outline"
      data-aspect={light.aspect}
      className="border-[color-mix(in_srgb,var(--aspect)_40%,transparent)] bg-[color-mix(in_srgb,var(--aspect)_8%,transparent)] font-normal text-[var(--aspect)]"
    >
      <span aria-hidden="true" className="size-1.5 rounded-full bg-[var(--aspect)]" />
      {t(light.label)}
    </Badge>
  );
}

/** Uma mensagem da conversa. O pedido vai num balão à direita, como numa
 * conversa; a resposta ocupa a coluna, ao lado da marca do JayV, para que
 * código e tabelas tenham onde caber. A cor que a portaria deu vira um selo
 * na resposta e, quando não é verde, também no pedido e tinge o que foi
 * dito. */
export function MessageBubble({ role, content, turn, meta, pending, onRetry, onOpenFile, children }: {
  role: "user" | "assistant";
  content: string;
  turn: TurnView | null;
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
  // Cada mensagem do agente no seu bloco, como o Claude e o Codex mostram:
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
      <article aria-label={t("chat.you")} className={cn("my-4 flex flex-col items-end gap-1.5 ps-[12%]", animate)}>
        {/* O pedido que passou limpo não precisa de selo: só o que a portaria
            barrou ou perguntou chama a atenção. */}
        {tinted && <LightBadge light={light} />}
        <div
          data-aspect={light?.aspect}
          className={cn(
            "max-w-full rounded-2xl rounded-ee-md border border-transparent bg-secondary px-4 py-2.5 leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere]",
            tinted && "border-[color-mix(in_srgb,var(--aspect)_45%,transparent)]",
          )}
        >
          {text}
        </div>
        {children}
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
    <article aria-label="JayV" className={cn("group/message my-4 flex gap-3", animate)}>
      <BrandMark className="mt-0.5 size-7 text-sm" />
      <div className="min-w-0 flex-1">
        <div className="flex min-h-7 flex-wrap items-center gap-x-2.5 gap-y-1 pb-1.5 text-xs">
          <strong className="font-semibold">JayV</strong>
          {route && (
            <small data-mode={route.mode ?? undefined} title={routeHint(route)} className="min-w-0 truncate text-caption text-muted-foreground data-[mode=build]:text-success">
              {routeLabel(route)}
            </small>
          )}
          {light && <span className="ms-auto"><LightBadge light={light} /></span>}
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
                    // O que o agente disse no caminho: discreto, numa trilha.
                    !last && "border-s-2 border-border ps-3 text-small text-muted-foreground",
                    last && tinted && "rounded-lg border border-[color-mix(in_srgb,var(--aspect)_40%,transparent)] bg-[color-mix(in_srgb,var(--aspect)_6%,transparent)] px-4 py-3",
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
          <div className="mt-1.5 flex min-h-6 items-center gap-2 text-caption text-muted-foreground">
            {answer && (
              <Button
                variant="ghost"
                size="icon-xs"
                onClick={() => void copy()}
                aria-label={t(copied ? "chat.copied" : "chat.copy")}
                title={t(copied ? "chat.copied" : "chat.copy")}
                className="text-muted-foreground opacity-0 transition-opacity group-hover/message:opacity-100 focus-visible:opacity-100 data-[copied=true]:opacity-100"
                data-copied={copied}
              >
                {copied ? <CheckIcon aria-hidden="true" /> : <CopyIcon aria-hidden="true" />}
              </Button>
            )}
            {meta && <small>{meta}</small>}
          </div>
        )}
        {pending && meta && <small className="mt-2 block text-caption text-muted-foreground">{meta}</small>}
      </div>
    </article>
  );
}
