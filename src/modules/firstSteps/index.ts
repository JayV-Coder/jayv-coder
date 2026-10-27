import { create } from "zustand";
import { supabase } from "@/modules/auth/client";
import { bus, onCore } from "@/modules/core";
import { useTelemetry } from "@/modules/telemetry";
import { useWorkspace } from "@/modules/workspace";
import { EMPTY, mark, parse, secondsToVerified, type FirstStep, type Progress } from "./progress";

export { FIRST_STEPS, complete, nextStep, secondsToVerified, showing, type FirstStep, type Progress } from "./progress";

/** O caminho guiado dos primeiros 5 minutos (roadmap 3.1). Mora neste
 * computador; o tempo até o primeiro resultado comprovado só sai daqui com a
 * telemetria ligada. */
const KEY = "jayv.firstSteps";

function load(): Progress {
  try { return parse(localStorage.getItem(KEY)); } catch { return EMPTY; }
}

export const useFirstSteps = create<Progress>(() => load());

function save(progress: Progress) {
  useFirstSteps.setState(progress, true);
  try { localStorage.setItem(KEY, JSON.stringify(progress)); } catch { /* sem armazenamento: vale só nesta sessão */ }
}

export function markStep(step: FirstStep) {
  const current = useFirstSteps.getState();
  const next = mark(current, step, Date.now());
  if (next !== current) save(next);
  if (step === "verified") void report();
}

export function dismissFirstSteps() {
  save({ ...useFirstSteps.getState(), dismissed: true });
}

/** Mostra o caminho de novo (pela ajuda ou pelo Ctrl+K). */
export function restartFirstSteps() {
  save({ ...EMPTY, startedAt: Date.now() });
}

/** Manda o tempo até o primeiro resultado, uma vez, com a telemetria ligada. */
async function report() {
  const progress = useFirstSteps.getState();
  const seconds = secondsToVerified(progress);
  if (progress.reported || seconds === null || useTelemetry.getState().granted !== true) return;
  const { error } = await supabase.rpc("record_first_result", { seconds });
  if (!error) save({ ...useFirstSteps.getState(), reported: true });
}

/** Ouve o app: a primeira pasta, o primeiro pedido e a primeira validação que
 * passou. Quem já tinha projetos antes do caminho existir não o vê. */
export function connectFirstSteps(): () => void {
  const fresh = (() => { try { return localStorage.getItem(KEY) === null; } catch { return false; } })();
  const offLoaded = bus.on("workspace:loaded", () => {
    const projects = useWorkspace.getState().data.projects.length;
    const state = useFirstSteps.getState();
    if (fresh && state.startedAt === null) {
      save(projects > 0 ? { ...state, dismissed: true } : { ...state, startedAt: Date.now() });
    }
  });
  const offProjects = useWorkspace.subscribe((state, previous) => {
    if (state.data.projects.length > previous.data.projects.length) markStep("folder");
  });
  const unlisten = [
    onCore("chat-prompt", () => markStep("request")),
    onCore("turn-beat", ({ kind, detail }) => {
      if (kind === "validate_end" && detail.verdict === "passed") markStep("verified");
    }),
  ];
  // A telemetria pode ligar depois do resultado: o tempo vai quando ligar.
  const offTelemetry = useTelemetry.subscribe((state) => { if (state.granted === true) void report(); });
  return () => {
    offLoaded();
    offProjects();
    offTelemetry();
    for (const off of unlisten) void off.then((stop) => stop());
  };
}
