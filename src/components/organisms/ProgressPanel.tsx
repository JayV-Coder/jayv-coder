import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
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

const WIDTH_KEY = "jayv.progress.width";
const MIN_WIDTH = 220;
const MAX_WIDTH = 640;
const KEY_STEP = 16;

/** Mantém a largura entre o mínimo do painel e o que ainda deixa lugar para a conversa. */
function clampWidth(width: number): number {
  const room = typeof window === "undefined" ? MAX_WIDTH : Math.max(MIN_WIDTH, window.innerWidth - 480);
  return Math.round(Math.min(Math.max(width, MIN_WIDTH), Math.min(MAX_WIDTH, room)));
}

/** A largura que a pessoa deixou, ou a de antes do ajuste (18 rem; 20 rem em janela larga). */
function rememberedWidth(): number {
  try {
    const saved = Number(localStorage.getItem(WIDTH_KEY));
    if (saved > 0) return clampWidth(saved);
    return clampWidth(window.matchMedia("(min-width: 1280px)").matches ? 320 : 288);
  } catch {
    return 288;
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
  const [width, setWidth] = useState(rememberedWidth);
  const dragging = useRef<{ x: number; width: number } | null>(null);
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

  // A janela encolhendo não deixa o painel comer a conversa.
  useEffect(() => {
    const fit = () => setWidth((value) => clampWidth(value));
    window.addEventListener("resize", fit);
    return () => window.removeEventListener("resize", fit);
  }, []);

  const save = (value: number) => {
    try { localStorage.setItem(WIDTH_KEY, String(value)); } catch { /* fica só nesta sessão */ }
  };
  const direction = () => (document.documentElement.dir === "rtl" ? -1 : 1);
  const startDrag = (event: PointerEvent<HTMLDivElement>) => {
    dragging.current = { x: event.clientX, width };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const drag = (event: PointerEvent<HTMLDivElement>) => {
    const start = dragging.current;
    if (start) setWidth(clampWidth(start.width + (event.clientX - start.x) * direction()));
  };
  const endDrag = () => {
    if (!dragging.current) return;
    dragging.current = null;
    setWidth((value) => { save(value); return value; });
  };
  const nudge = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.key === "ArrowRight" ? KEY_STEP * direction() : event.key === "ArrowLeft" ? -KEY_STEP * direction() : 0;
    if (!step) return;
    event.preventDefault();
    const next = clampWidth(width + step);
    setWidth(next);
    save(next);
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
    <aside aria-label={t("progress.title")} style={{ width }} className="relative flex min-h-0 flex-none flex-col border-e border-border bg-card font-mono text-small">
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
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label={t("progress.resize")}
        aria-valuemin={MIN_WIDTH}
        aria-valuemax={MAX_WIDTH}
        aria-valuenow={width}
        tabIndex={0}
        title={t("progress.resize")}
        onPointerDown={startDrag}
        onPointerMove={drag}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onKeyDown={nudge}
        onDoubleClick={() => { const initial = clampWidth(320); setWidth(initial); try { localStorage.removeItem(WIDTH_KEY); } catch { /* ignora */ } }}
        className="absolute inset-y-0 -end-1 z-10 w-2 cursor-col-resize touch-none select-none transition-colors hover:bg-primary/30 focus-visible:bg-primary/40 focus-visible:outline-none active:bg-primary/50"
      />
    </aside>
  );
}
