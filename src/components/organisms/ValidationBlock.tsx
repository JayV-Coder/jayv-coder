import { useState } from "react";
import { CheckCircle2Icon, CircleDashedIcon, CircleSlashIcon, Undo2Icon, XCircleIcon } from "lucide-react";
import type { Activity } from "@/modules/core";
import { reportError, notify } from "@/modules/feedback";
import { useT, type Key } from "@/modules/i18n";
import { lacksTest, rollbackCheckpoint, stepDuration, validationOf, type StepStatus, type ValidationStep } from "@/modules/validation";
import { ChevronIcon } from "@/components/atoms";
import { ConfirmAction } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const PASSED: StepStatus[] = ["passed"];

function StepIcon({ status }: { status: StepStatus }) {
  if (PASSED.includes(status)) return <CheckCircle2Icon aria-hidden="true" className="size-3.5 text-go" />;
  if (status === "pending") return <CircleDashedIcon aria-hidden="true" className="size-3.5 text-muted-foreground" />;
  if (status === "blocked" || status === "stopped") return <CircleSlashIcon aria-hidden="true" className="size-3.5 text-warning" />;
  return <XCircleIcon aria-hidden="true" className="size-3.5 text-stop" />;
}

/** Uma etapa da validação: o resultado, o comando e, se não passou, o fim da saída. */
function StepRow({ step }: { step: ValidationStep }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const detail = step.tail || (step.blockedBy.length > 0 ? t("validation.blockedBy", { orgs: step.blockedBy.join(", ") }) : "");
  return (
    <li className="grid gap-1">
      <div className="flex min-w-0 items-center gap-2 text-small">
        <StepIcon status={step.status} />
        <span className="font-medium">{t(`validation.kind.${step.kind}`)}</span>
        <code className="min-w-0 flex-1 truncate text-caption text-muted-foreground" title={step.command}>{step.command}</code>
        <small className="shrink-0 text-caption text-muted-foreground">
          {t(`validation.status.${step.status}`)}{step.status !== "pending" && step.millis > 0 ? ` · ${stepDuration(step.millis)}` : ""}
        </small>
        {detail && (
          <button
            type="button"
            aria-expanded={open}
            aria-label={t("validation.output")}
            onClick={() => setOpen(!open)}
            className="rounded-xs focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          >
            <ChevronIcon className={cn("w-[14px] flex-none transition-transform duration-200 motion-reduce:transition-none", !open && "-rotate-90")} />
          </button>
        )}
      </div>
      {detail && open && <pre className="max-h-64 overflow-auto rounded-sm bg-secondary/60 p-2 text-caption whitespace-pre-wrap [overflow-wrap:anywhere]">{detail}</pre>}
    </li>
  );
}

/** O que o validador provou sobre o build: etapa por etapa, o limite de
 * reparos, a volta ao estado anterior e o que a organização reteve. */
export function ValidationBlock({ events, turnId, canRollback }: { events: Activity[]; turnId: string; canRollback: boolean }) {
  const t = useT();
  const state = validationOf(events);
  const [busy, setBusy] = useState(false);
  if (!state) return null;
  const missing = lacksTest(state);
  const verdictKey: Key = state.skipped ? "validation.skipped" : state.verdict ? (`validation.verdict.${state.verdict}` as Key) : "validation.running";

  const back = async () => {
    setBusy(true);
    try {
      const result = await rollbackCheckpoint(turnId);
      notify(t(result.headMoved ? "validation.rolledBack.moved" : "validation.rolledBack", { count: result.files }));
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section aria-label={t("validation.title")} className="grid gap-2 rounded-md border border-border bg-secondary/30 px-3 py-2">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <strong className="text-small">{t("validation.title")}</strong>
        <span data-verdict={state.verdict ?? undefined} className="text-small data-[verdict=failed]:text-stop data-[verdict=passed]:text-go">{t(verdictKey)}</span>
        {state.maxRepairs > 0 && (state.repairs > 0 || state.verdict === "failed") && (
          <small className="text-caption text-muted-foreground">{t("validation.repairs", { used: state.repairs, max: state.maxRepairs })}</small>
        )}
      </div>
      {state.steps.length > 0 && <ul className="grid gap-1.5">{state.steps.map((step) => <StepRow key={step.kind} step={step} />)}</ul>}
      {!state.skipped && missing && <p className="text-caption text-warning">{t("validation.noTest")}</p>}
      {state.held && <p className="text-caption text-stop">{t("validation.held", { commands: state.held.commands.join(", "), orgs: state.held.orgs.join(", ") })}</p>}
      {state.rolledBack ? (
        <p className="text-caption text-muted-foreground">{t("validation.rolledBack.done", { count: state.rolledBack.files })}</p>
      ) : (
        state.checkpoint && canRollback && (
          <div>
            <ConfirmAction title={t("validation.rollback.title")} description={t("validation.rollback.description")} confirm={t("validation.rollback")} onConfirm={() => void back()}>
              <Button variant="outline" size="xs" disabled={busy} title={t("validation.rollback.hint")}>
                <Undo2Icon aria-hidden="true" />
                {t("validation.rollback")}
              </Button>
            </ConfirmAction>
          </div>
        )
      )}
    </section>
  );
}
