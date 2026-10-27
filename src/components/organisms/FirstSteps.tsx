import { useState } from "react";
import { CheckIcon } from "lucide-react";
import { dismissFirstSteps, FIRST_STEPS, nextStep, secondsToVerified, showing, useFirstSteps, type FirstStep } from "@/modules/firstSteps";
import { reportError } from "@/modules/feedback";
import { useT, type Key } from "@/modules/i18n";
import { saveCoreField, useSettings } from "@/modules/settings";
import { createChat, useWorkspace } from "@/modules/workspace";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { NewProjectDialog } from "./NewProjectDialog";

/** Os primeiros 5 minutos: abrir a pasta, fazer o primeiro pedido e ver o
 * primeiro resultado comprovado, sem abrir as Configurações (a comprovação
 * liga aqui mesmo). Some quando acaba ou quando a pessoa fecha; no fim, diz
 * quanto tempo levou. */
export function FirstSteps({ className }: { className?: string }) {
  const t = useT();
  const progress = useFirstSteps();
  const projects = useWorkspace((state) => state.data.projects);
  const validating = useSettings((state) => state.core?.validateBuilds ?? false);
  const [busy, setBusy] = useState(false);
  const [justDone] = useState(() => progress.done.verified !== undefined && !progress.dismissed);
  const next = nextStep(progress);
  const seconds = secondsToVerified(progress);

  if (!showing(progress)) {
    // Acabou agora: um aviso só, com o tempo, até a pessoa fechar.
    if (progress.dismissed || seconds === null || justDone) return null;
  }

  const turnOnValidation = async () => {
    setBusy(true);
    try { await saveCoreField({ validateBuilds: true }); } catch (error) { reportError(error); } finally { setBusy(false); }
  };

  const action = (step: FirstStep) => {
    if (step !== next) return null;
    if (step === "folder") return <NewProjectDialog><Button size="sm">{t("firstSteps.folder.action")}</Button></NewProjectDialog>;
    if (step === "request") {
      const project = projects[0];
      return project ? <Button size="sm" onClick={() => void createChat(project.id)}>{t("firstSteps.request.action")}</Button> : null;
    }
    return validating
      ? <span className="text-xs text-muted-foreground">{t("firstSteps.verified.waiting")}</span>
      : <Button size="sm" loading={busy} onClick={() => void turnOnValidation()}>{t("firstSteps.verified.action")}</Button>;
  };

  return (
    <section data-tour="first-steps" aria-labelledby="first-steps-title" className={cn("rounded-lg border border-primary/40 bg-primary/5 px-4 py-4", className)}>
      <div className="mb-3 flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <h2 id="first-steps-title" className="text-sm font-semibold">{t("firstSteps.title")}</h2>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {next === null && seconds !== null ? t("firstSteps.done", { minutes: Math.max(1, Math.round(seconds / 60)) }) : t("firstSteps.hint")}
          </p>
        </div>
        <Button variant="ghost" size="sm" onClick={dismissFirstSteps}>{t(next === null ? "common.close" : "firstSteps.skip")}</Button>
      </div>
      <ol className="grid gap-2">
        {FIRST_STEPS.map((step, index) => {
          const done = progress.done[step] !== undefined;
          return (
            <li key={step} className={cn("flex flex-wrap items-center gap-3 rounded-md border px-3 py-2", step === next ? "border-border bg-card" : "border-transparent")}>
              <span aria-hidden="true" className={cn("flex size-6 shrink-0 items-center justify-center rounded-full border text-xs font-semibold",
                done ? "border-success bg-success text-background" : "border-border text-muted-foreground")}>
                {done ? <CheckIcon className="size-3.5" /> : index + 1}
              </span>
              <div className="min-w-0 flex-1">
                <p className={cn("text-sm", done && "text-muted-foreground line-through")}>{t(`firstSteps.${step}.title` as Key)}</p>
                {step === next && <p className="text-xs text-muted-foreground">{t(`firstSteps.${step}.hint` as Key)}</p>}
              </div>
              {action(step)}
              <span className="sr-only">{t(done ? "firstSteps.state.done" : "firstSteps.state.todo")}</span>
            </li>
          );
        })}
      </ol>
    </section>
  );
}
