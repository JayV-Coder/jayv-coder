import { useEffect } from "react";
import { create } from "zustand";
import { bus, type View } from "@/modules/core";
import { useI18n } from "@/modules/i18n";
import { MANUAL } from "./manual";
import { TOURS, tourForView, tourOf, type Tour } from "./tours";

export { TOURS, tourForView, tourOf, MANUAL };
export type { Tour, TourStep } from "./tours";

const KEY = "jayv.tutorial";

interface Saved { seen: string[]; auto: boolean }

function load(): Saved {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "null") as Partial<Saved> | null;
    return { seen: Array.isArray(saved?.seen) ? saved.seen.filter((id) => typeof id === "string") : [], auto: saved?.auto !== false };
  } catch { return { seen: [], auto: true }; }
}

function save({ seen, auto }: Saved) {
  try { localStorage.setItem(KEY, JSON.stringify({ seen, auto })); } catch { /* sem armazenamento: vale só nesta sessão */ }
}

interface TutorialState extends Saved {
  /** O tutorial em curso e o passo em que está. */
  active: { tour: string; step: number } | null;
}

export const useTutorial = create<TutorialState>(() => ({ ...load(), active: null }));

const persist = () => { const { seen, auto } = useTutorial.getState(); save({ seen, auto }); };

/** Abre o tutorial pelo começo (o botão, a paleta e a primeira visita). */
export function startTour(id: string) {
  if (!tourOf(id)) return;
  useTutorial.setState({ active: { tour: id, step: 0 } });
}

/** Abre o tutorial da tela aberta, quando há um. */
export function startTourHere(view: View) {
  const tour = tourForView(view);
  if (tour) startTour(tour.id);
}

export function nextStep() {
  const { active } = useTutorial.getState();
  const tour = active && tourOf(active.tour);
  if (!active || !tour) return;
  if (active.step + 1 >= tour.steps.length) endTour();
  else useTutorial.setState({ active: { tour: active.tour, step: active.step + 1 } });
}

export function previousStep() {
  const { active } = useTutorial.getState();
  if (active && active.step > 0) useTutorial.setState({ active: { tour: active.tour, step: active.step - 1 } });
}

/** Fecha o tutorial e o marca como visto: não abre sozinho de novo. */
export function endTour() {
  const { active, seen } = useTutorial.getState();
  if (!active) return;
  useTutorial.setState({ active: null, seen: seen.includes(active.tour) ? seen : [...seen, active.tour] });
  persist();
}

/** "Não mostrar mais": todos viram vistos e nenhum abre sozinho; os botões
 * e a paleta continuam abrindo qualquer um. */
export function skipAllTours() {
  useTutorial.setState({ active: null, auto: false, seen: TOURS.map((tour) => tour.id) });
  persist();
}

export function setAutoTours(auto: boolean) {
  useTutorial.setState({ auto });
  persist();
}

/** Esquece o que foi visto: cada tela volta a abrir o seu tutorial na próxima visita. */
export function resetTours() {
  useTutorial.setState({ seen: [], active: null });
  persist();
}

/** O texto de um passo: a tradução da documentação, ou o inglês do manual. */
export function stepText(feature: string, part: "title" | "summary" | "usage", messages: Record<string, unknown>): string {
  const translated = messages[`docs.${feature}.${part}`];
  return typeof translated === "string" ? translated : MANUAL[feature]?.[part] ?? "";
}

/** Na primeira visita a uma tela, o tutorial dela abre sozinho (um por vez,
 * e só com a pessoa logada e nada mais na frente). */
export function connectTutorial(blocked: () => boolean) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const offer = (view: View) => {
    clearTimeout(timer);
    const state = useTutorial.getState();
    const tour: Tour | undefined = tourForView(view);
    if (!tour || !state.auto || state.active || state.seen.includes(tour.id)) return;
    // Espera a tela montar (e a janela de novidades ou de atualização sair da frente).
    timer = setTimeout(() => { if (!blocked() && !useTutorial.getState().active) startTour(tour.id); }, 900);
  };
  const off = bus.on("view:changed", ({ view }) => offer(view));
  return () => { clearTimeout(timer); off(); };
}

/** Os textos do idioma atual, para quem monta o passo. */
export function useStepText() {
  const messages = useI18n((state) => state.messages) as Record<string, unknown>;
  return (feature: string, part: "title" | "summary" | "usage") => stepText(feature, part, messages);
}

/** Fecha o tutorial com Esc e navega com as setas, enquanto ele está aberto. */
export function useTutorialKeys(active: boolean) {
  useEffect(() => {
    if (!active) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.stopPropagation(); endTour(); }
      else if (event.key === "ArrowRight") nextStep();
      else if (event.key === "ArrowLeft") previousStep();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [active]);
}
