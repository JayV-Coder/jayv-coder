import type { ReactNode } from "react";
import type { TurnView } from "@/modules/core";
import { messageLight, routeHint, routeLabel, shownText } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { Markdown } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** Onde uma mensagem do agente termina e a próxima começa (o
 * `MESSAGE_BREAK` do núcleo). */
const MESSAGE_BREAK = "\u2063";

function splitMessages(text: string): string[] {
  const parts = text.split(MESSAGE_BREAK).map((part) => part.trim()).filter(Boolean);
  return parts.length > 0 ? parts : [text];
}

/** Um balão da conversa. O pedido e a resposta levam a cor que a portaria deu
 * ao pedido. */
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
  const light = turn ? messageLight(role, turn) : null;
  const user = role === "user";
  // Pedido barrado, falha e resposta a uma pergunta ficam gravados como aviso
  // e são ditos aqui no idioma de quem lê.
  const text = shownText(content);
  // A resposta diz quem a escreveu: o agente, o modelo, o modo e o papel que
  // o Jev escolheu para o pedido.
  const route = !user && turn?.route ? turn.route : null;
  // Cada mensagem do agente no seu balão, como o Claude e o Codex mostram:
  // o que ele avisou no caminho não se mistura com a resposta final.
  const parts = user ? [text] : splitMessages(text);
  const bubble = cn(
    "rounded-[13px] border border-border bg-[#151916] px-[19px] py-[17px] leading-[1.58] [overflow-wrap:anywhere]",
    user && "border-[#324a37] bg-[#1d2920] whitespace-pre-wrap",
    pending && "border-dashed whitespace-pre-wrap",
    light && "border-[var(--aspect)] shadow-[0_0_0_1px_color-mix(in_srgb,var(--aspect)_22%,transparent),0_6px_20px_-14px_var(--glow)]",
  );
  return (
    <article className={cn("my-[22px]", user ? "ms-[16%]" : "me-[10%]", pending && "animate-pending-in motion-reduce:animate-none")}>
      <div className="flex items-baseline gap-2.5 px-1 pb-2 text-xs">
        <strong className={route ? undefined : "me-auto"}>{user ? t("chat.you") : "JayV"}</strong>
        {route && (
          <small data-mode={route.mode ?? undefined} title={routeHint(route)} className="me-auto truncate text-[#8fa394] data-[mode=build]:text-[#a4f4a9]">
            {routeLabel(route)}
          </small>
        )}
        {light && <small data-aspect={light.aspect} className="text-[var(--aspect)]">{t(light.label)}</small>}
        {meta && <small className="text-[#6e7870]">{meta}</small>}
      </div>
      {(text || !pending) && (
        <div className="grid gap-2">
          {parts.map((part, index) => (
            <div key={index} data-aspect={light?.aspect} className={cn(bubble, index < parts.length - 1 && "border-border shadow-none text-[#c3cdc5]")}>
              {user || pending ? part : <Markdown content={part} onOpenFile={onOpenFile} />}
            </div>
          ))}
        </div>
      )}
      {children}
      {user && turn?.status === "failed" && onRetry && (
        <div className="flex items-center gap-2.5 px-1 pt-2">
          <Button variant="outline" size="xs" title={t("chat.retry.title")} onClick={onRetry}>{t("chat.retry")}</Button>
          <small className="text-xs text-[#6e7870]">{t("chat.retry.note")}</small>
        </div>
      )}
    </article>
  );
}
