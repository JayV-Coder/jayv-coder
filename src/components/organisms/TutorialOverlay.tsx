import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useT } from "@/modules/i18n";
import { endTour, nextStep, previousStep, skipAllTours, tourOf, useStepText, useTutorial, useTutorialKeys } from "@/modules/tutorial";
import { Button } from "@/components/ui/button";

const GAP = 14;
const PAD = 6;
const MARGIN = 12;

interface Box { top: number; left: number; width: number; height: number }

/** Mede o elemento destacado e o remede quando a janela muda ou a tela se
 * mexe; sem o elemento (não existe nesta tela, ou está escondido), o passo
 * abre no centro. */
function useTarget(selector: string | undefined): Box | null {
  const [box, setBox] = useState<Box | null>(null);
  useEffect(() => {
    if (!selector) { setBox(null); return; }
    let frame = 0;
    const measure = () => {
      const element = document.querySelector<HTMLElement>(`[data-tour="${selector}"]`);
      const rect = element?.getBoundingClientRect();
      setBox(rect && rect.width > 0 && rect.height > 0 ? { top: rect.top, left: rect.left, width: rect.width, height: rect.height } : null);
    };
    document.querySelector<HTMLElement>(`[data-tour="${selector}"]`)?.scrollIntoView({ block: "nearest", inline: "nearest" });
    measure();
    const tick = () => { measure(); frame = requestAnimationFrame(tick); };
    // A tela pode mexer sozinha (animação, rolagem): acompanha enquanto o passo está aberto.
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [selector]);
  return box;
}

/** O tutorial na frente da tela: um cartão com o que a funcionalidade faz e
 * como se usa (o texto da documentação) e, quando o passo aponta para um
 * elemento, o destaque em volta dele. Esc fecha; as setas navegam. */
export function TutorialOverlay() {
  const t = useT();
  const text = useStepText();
  const active = useTutorial((state) => state.active);
  useTutorialKeys(active !== null);
  const card = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 360, height: 220 });
  const tour = active ? tourOf(active.tour) : undefined;
  const step = tour && active ? tour.steps[active.step] : undefined;
  const box = useTarget(step?.target);

  useLayoutEffect(() => {
    const element = card.current;
    if (element) setSize((previous) => previous.width === element.offsetWidth && previous.height === element.offsetHeight ? previous : { width: element.offsetWidth, height: element.offsetHeight });
  });

  if (!active || !tour || !step) return null;
  const last = active.step === tour.steps.length - 1;
  const viewport = { width: window.innerWidth, height: window.innerHeight };
  const clamp = (value: number, max: number) => Math.max(MARGIN, Math.min(value, max - MARGIN));

  let style: React.CSSProperties;
  if (box) {
    const right = box.left + box.width + PAD + GAP;
    const below = box.top + box.height + PAD + GAP;
    if (right + size.width + MARGIN <= viewport.width) style = { left: right, top: clamp(box.top, viewport.height - size.height) };
    else if (below + size.height + MARGIN <= viewport.height) style = { top: below, left: clamp(box.left, viewport.width - size.width) };
    else style = { top: clamp(box.top - PAD - GAP - size.height, viewport.height - size.height), left: clamp(box.left, viewport.width - size.width) };
  } else {
    style = { top: Math.max(MARGIN, (viewport.height - size.height) / 2), left: Math.max(MARGIN, (viewport.width - size.width) / 2) };
  }

  return (
    <div className="fixed inset-0 z-[70]">
      {/* Nada por trás recebe clique enquanto o tutorial está aberto. */}
      <div aria-hidden="true" className={box ? "absolute inset-0" : "absolute inset-0 bg-black/55"} />
      {box && (
        <div aria-hidden="true" className="pointer-events-none absolute rounded-md ring-2 ring-accent transition-[top,left,width,height] duration-150 motion-reduce:transition-none"
          style={{ top: box.top - PAD, left: box.left - PAD, width: box.width + PAD * 2, height: box.height + PAD * 2, boxShadow: "0 0 0 9999px rgb(0 0 0 / 0.55)" }} />
      )}
      <div ref={card} role="dialog" aria-modal="true" aria-labelledby="tutorial-title" style={{ ...style, width: "min(24rem, calc(100vw - 1.5rem))" }}
        className="absolute grid gap-3 rounded-lg border border-border bg-popover p-4 text-popover-foreground shadow-lg">
        <div className="flex items-center justify-between gap-2 font-mono text-caption text-muted-foreground">
          <span>{t(`tutorial.tour.${tour.id}` as never)}</span>
          <span className="tabular-nums">{t("tutorial.step", { current: active.step + 1, total: tour.steps.length })}</span>
        </div>
        <div className="grid gap-1.5">
          <h2 id="tutorial-title" className="text-base font-semibold">{text(step.feature, "title")}</h2>
          <p className="text-sm">{text(step.feature, "summary")}</p>
          <p className="text-sm text-muted-foreground">{text(step.feature, "usage")}</p>
        </div>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex gap-1">
            <Button size="sm" variant="ghost" onClick={endTour}>{t("tutorial.skip")}</Button>
            <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={skipAllTours}>{t("tutorial.skipAll")}</Button>
          </div>
          <div className="flex gap-1.5">
            {active.step > 0 && <Button size="sm" variant="outline" onClick={previousStep}>{t("tutorial.back")}</Button>}
            <Button size="sm" autoFocus onClick={nextStep}>{t(last ? "tutorial.done" : "tutorial.next")}</Button>
          </div>
        </div>
      </div>
    </div>
  );
}
