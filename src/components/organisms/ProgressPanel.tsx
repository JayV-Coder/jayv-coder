import { useEffect, useRef, useState, type ReactNode } from "react";
import { PanelLeftCloseIcon, PanelLeftOpenIcon } from "lucide-react";
import type { Chat } from "@/modules/core";
import { beatLines, liveOf, pendingWord, useConversation } from "@/modules/conversation";
import { openTurns } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { PulseDot } from "@/components/atoms";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const OPEN_KEY = "jayv.progress.open";

/** Aberto ou recolhido como a pessoa deixou; sem escolha, abre se a janela
 * tem largura para a conversa e o painel lado a lado. */
function remembered(): boolean {
  try {
    const saved = localStorage.getItem(OPEN_KEY);
    if (saved !== null) return saved !== "0";
    return window.matchMedia("(min-width: 1024px)").matches;
  } catch {
    return true;
  }
}

/** O painel à esquerda do chat, junto ao menu lateral: o andamento do JayV,
 * com cada etapa do Jev e da portaria numa linha só dela, e, embaixo, o que
 * mais acompanha o chat (os repositórios da organização, em `children`).
 * Assim o balão do chat fica só com a resposta. Recolhido, vira uma faixa
 * fina que ainda mostra se há pedido em andamento. */
export function ProgressPanel({ chat, children }: { chat: Chat | null; children?: ReactNode }) {
  const t = useT();
  const steps = useRef<HTMLOListElement>(null);
  const [open, setOpen] = useState(remembered);
  const turn = openTurns(chat)[0];
  const queued = openTurns(chat).length - 1;
  const live = useConversation((state) => (turn ? state.live[turn.id] : undefined));
  const { text, beats } = turn ? liveOf(turn, live) : { text: "", beats: [] };
  const lines = beatLines(beats);
  // A última etapa é a de agora e vai destacada embaixo; a resposta chegando
  // fecha todas as etapas.
  const done = text ? lines : lines.slice(0, -1);

  useEffect(() => {
    const element = steps.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [done.length, turn?.id, open]);

  const toggle = () => {
    setOpen((value) => {
      try { localStorage.setItem(OPEN_KEY, value ? "0" : "1"); } catch { /* fica só nesta sessão */ }
      return !value;
    });
  };

  if (!open) {
    return (
      <aside aria-label={t("progress.title")} className="flex min-h-0 w-10 flex-none flex-col items-center gap-3 border-e border-border bg-card py-2 font-mono">
        <Button size="icon-xs" variant="ghost" aria-expanded={false} aria-label={t("progress.expand")} title={t("progress.expand")} onClick={toggle}>
          <PanelLeftOpenIcon aria-hidden="true" />
        </Button>
        {turn && <PulseDot />}
      </aside>
    );
  }

  return (
    <aside aria-label={t("progress.title")} className="flex min-h-0 w-72 flex-none flex-col border-e border-border bg-card font-mono text-small xl:w-80">
      <header className="flex flex-none items-center gap-2 border-b border-border px-3 py-1.5">
        <span aria-hidden="true" className="text-primary">❯</span>
        <span className="text-foreground">{t("progress.title")}</span>
        <span className="ms-auto" />
        <Button size="icon-xs" variant="ghost" aria-expanded aria-label={t("progress.collapse")} title={t("progress.collapse")} onClick={toggle}>
          <PanelLeftCloseIcon aria-hidden="true" />
        </Button>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto" role="status" aria-live="polite">
        {!turn ? (
          <p className="px-3 py-3 text-muted-foreground">{t("progress.idle")}</p>
        ) : (
          <div className="animate-pending-in px-3 py-2.5 motion-reduce:animate-none">
            <div className="flex items-baseline gap-2">
              <PulseDot />
              <strong className="text-xs font-semibold text-foreground"><span aria-hidden="true" className="me-1.5">🤖</span>JayV</strong>
              <span className="ms-auto text-caption text-muted-foreground">{queued > 0 ? t("pending.queued", { count: queued }) : t("pending.running")}</span>
            </div>
            <ol ref={steps} className="mt-2.5 grid gap-1.5">
              {done.map((line) => (
                <li
                  key={line.seq}
                  data-kind={line.kind}
                  className="flex gap-2 rounded-xs border border-border bg-background px-2 py-1.5 text-xs leading-[1.45] text-muted-foreground data-[kind=failed]:border-destructive/50 data-[kind=failed]:text-destructive"
                >
                  <span aria-hidden="true" className={cn("flex-none", line.kind === "failed" ? "text-destructive" : "text-go")}>{line.kind === "failed" ? "✕" : "✓"}</span>
                  <span className="min-w-0 break-words">{line.line}</span>
                </li>
              ))}
              <li className="flex gap-2 rounded-xs border border-primary/50 bg-background px-2 py-1.5 text-xs leading-[1.45]">
                <span aria-hidden="true" className="flex-none text-primary">❯</span>
                <span className="shimmer-text min-w-0 break-words">{pendingWord(text, beats)}</span>
              </li>
            </ol>
          </div>
        )}
      </div>
      {children}
    </aside>
  );
}
