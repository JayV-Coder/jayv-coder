import { useEffect, useState, type ReactNode } from "react";
import type { ValidationCommands, ValidationSetup } from "@/modules/core";
import { useIntentHandler } from "@/modules/commands";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { loadValidationSetup, saveValidationCommands, STEP_KINDS } from "@/modules/validation";
import { Eyebrow } from "@/components/atoms";
import { FormField } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

const EMPTY: ValidationCommands = { test: "", lint: "", typecheck: "", build: "" };

/** Os comandos que o validador roda no projeto do chat: o que o JayV achou na
 * pasta e, por etapa, o que a pessoa escreveu no lugar (`off` pula a etapa). */
export function ValidationDialog({ chatId, children }: { chatId: string; children: ReactNode }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [setup, setSetup] = useState<ValidationSetup | null>(null);
  const [manual, setManual] = useState<ValidationCommands>(EMPTY);
  useIntentHandler("projectValidation", () => setOpen(true));

  useEffect(() => {
    if (!open) return;
    loadValidationSetup(chatId).then((loaded) => { setSetup(loaded); setManual(loaded.manual); }, reportError);
  }, [open, chatId]);

  const save = async () => {
    try {
      const saved = await saveValidationCommands(chatId, manual);
      setSetup(saved);
      setManual(saved.manual);
      setOpen(false);
    } catch (error) {
      reportError(error);
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>{children}</DialogTrigger>
      <DialogContent className="grid max-h-[85vh] gap-3 overflow-y-auto sm:max-w-[640px]">
        <DialogHeader>
          <Eyebrow>{t("validation.eyebrow")}</Eyebrow>
          <DialogTitle className="text-xl">{t("validation.dialog.title")}</DialogTitle>
          <DialogDescription>{t("validation.dialog.description")}</DialogDescription>
        </DialogHeader>
        {STEP_KINDS.map((kind) => {
          const found = setup?.detected.steps.find((step) => step.kind === kind)?.command;
          return (
            <FormField key={kind} label={t(`validation.kind.${kind}`)} htmlFor={`validation-${kind}`} hint={found ? t("validation.detected", { command: found }) : t("validation.undefined")}>
              <Input id={`validation-${kind}`} value={manual[kind]} placeholder={found ?? ""} onChange={(event) => setManual({ ...manual, [kind]: event.target.value })} />
            </FormField>
          );
        })}
        <p className="text-caption text-muted-foreground">{t("validation.dialog.off")}</p>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setManual(EMPTY)}>{t("validation.dialog.reset")}</Button>
          <Button onClick={() => void save()}>{t("validation.dialog.save")}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
