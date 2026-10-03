import { CheckIcon, LoaderCircleIcon, XIcon } from "lucide-react";
import { useLocale, useT, type Key } from "@/modules/i18n";
import { checkForUpdate, closeUpdate, installUpdate, isUpdateBusy, useUpdate, type UpdatePhase, type UpdateStep as Step } from "@/modules/updates";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

const STEPS: Step[] = ["checking", "downloading", "installing", "restarting"];

/** Onde cada passo está: feito, em curso, à frente ou o que falhou. */
function stepState(step: Step, phase: UpdatePhase, failedAt: Step | null): "done" | "active" | "waiting" | "failed" {
  if (failedAt === step) return "failed";
  if (phase === "latest" || phase === "available") return step === "checking" ? "done" : "waiting";
  const at = failedAt ?? (STEPS.includes(phase as Step) ? (phase as Step) : "checking");
  const index = STEPS.indexOf(step);
  const current = STEPS.indexOf(at);
  if (index < current) return "done";
  return index === current ? "active" : "waiting";
}

/** A atualização acontecendo na frente de quem usa: de que versão para qual,
 * onde ver o que muda, cada passo com o seu estado e o download crescendo. Com a
 * versão nova achada, a janela pergunta antes: atualizar agora ou deixar para
 * depois. Só fecha quando não há nada no meio do caminho. */
export function UpdateDialog() {
  const t = useT();
  const locale = useLocale();
  const { open, phase, current, next, received, total, error } = useUpdate();
  const failedAt = useUpdate((state) => (state.phase === "failed" ? state.failedAt : null));
  const busy = isUpdateBusy(phase);
  const megabytes = (bytes: number) => new Intl.NumberFormat(locale, { maximumFractionDigits: 1, minimumFractionDigits: 1 }).format(bytes / 1_048_576);
  const share = total ? Math.min(1, received / total) : null;

  const title = phase === "latest" ? t("update.latest") : phase === "available" && next ? t("update.available.title", { version: next }) : next ? t("update.title", { version: next }) : t("update.checking");
  return (
    <Dialog open={open} onOpenChange={(wanted) => { if (!wanted) closeUpdate(); }}>
      <DialogContent showCloseButton={!busy} className="sm:max-w-[520px]"
        onEscapeKeyDown={(event) => busy && event.preventDefault()} onPointerDownOutside={(event) => busy && event.preventDefault()}>
        <DialogHeader>
          <DialogTitle className="text-xl">{title}</DialogTitle>
          <DialogDescription>
            {next && current ? t(phase === "available" ? "update.available.fromTo" : "update.fromTo", { current, next }) : t("update.description")}
          </DialogDescription>
        </DialogHeader>

        {phase === "available" ? <p className="text-sm">{t("update.available.ask")}</p> : (
          <ol className="grid gap-2.5" aria-live="polite">
            {STEPS.map((step) => {
              const state = stepState(step, phase, failedAt);
              return (
                <li key={step} data-state={state} className="flex items-center gap-2.5 text-sm text-muted-foreground data-[state=active]:text-foreground data-[state=done]:text-foreground data-[state=failed]:text-destructive">
                  <span className="grid size-5 place-items-center">
                    {state === "done" && <CheckIcon aria-hidden="true" className="size-4 text-success" />}
                    {state === "active" && <LoaderCircleIcon aria-hidden="true" className="size-4 animate-spin motion-reduce:animate-none" />}
                    {state === "failed" && <XIcon aria-hidden="true" className="size-4" />}
                    {state === "waiting" && <span aria-hidden="true" className="size-1.5 rounded-full bg-muted-foreground/50" />}
                  </span>
                  {t(`update.step.${step}` as Key)}
                </li>
              );
            })}
          </ol>
        )}

        {(phase === "downloading" || (failedAt === "downloading" && received > 0)) && (
          <div className="grid gap-1.5">
            <div role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={share === null ? undefined : Math.round(share * 100)}
              className="h-2 overflow-hidden rounded-full bg-secondary">
              <div className={cn("h-full rounded-full bg-primary transition-[width]", share === null && "w-1/3 animate-pulse motion-reduce:animate-none")}
                style={share === null ? undefined : { width: `${share * 100}%` }} />
            </div>
            <small className="font-mono text-xs text-muted-foreground">
              {total ? t("update.progress", { received: megabytes(received), total: megabytes(total), percent: Math.round((share ?? 0) * 100) }) : t("update.progress.unknown", { received: megabytes(received) })}
            </small>
          </div>
        )}

        {busy && phase !== "checking" && <p className="text-xs text-muted-foreground">{t(phase === "restarting" ? "update.restarting.note" : "update.keepOpen")}</p>}

        {/* O corpo do release no GitHub são os assuntos dos commits, num idioma só;
            a lista traduzida chega com a versão nova, na janela Novidades. */}
        {next && (phase === "available" || busy) && (
          <div className="grid gap-1.5">
            <strong className="text-xs tracking-wider text-muted-foreground uppercase">{t("update.notes")}</strong>
            <p className="text-sm text-muted-foreground">{t("update.notes.after")}</p>
          </div>
        )}

        {phase === "failed" && error && <p role="alert" className="text-sm text-destructive [overflow-wrap:anywhere]">{t("update.failed", { error })}</p>}

        {!busy && (
          <DialogFooter>
            <Button variant="outline" onClick={closeUpdate}>{t(phase === "available" ? "update.later" : "common.close")}</Button>
            {phase === "failed" && <Button onClick={() => void checkForUpdate(true)}>{t("update.retry")}</Button>}
            {phase === "available" && <Button onClick={() => void installUpdate()}>{t("update.install")}</Button>}
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
