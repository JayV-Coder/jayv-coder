import { useState, type FormEvent, type ReactNode } from "react";
import { CloudIcon, TerminalIcon } from "lucide-react";
import { useIntentHandler } from "@/modules/commands";
import { useT, type Key } from "@/modules/i18n";
import { createMod, EMPTY_MOD, lineProblem, setSettingsTab, splitLine, type ModDraft } from "@/modules/settings";
import { Eyebrow } from "@/components/atoms";
import { FormField, SegmentedControl, ToggleRow } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

/** O que está faltando ou errado no formulário, campo por campo. */
function draftProblems(draft: ModDraft): Partial<Record<keyof ModDraft, Key>> {
  const found: Partial<Record<keyof ModDraft, Key>> = {};
  if (!draft.name.trim()) found.name = "mods.name.empty";
  if (draft.kind === "cli") {
    if (!draft.command.trim()) found.command = "agent.command.empty";
    else if (/\s/.test(draft.command.trim())) found.command = "agent.command.hint";
    const args = lineProblem(splitLine(draft.args));
    if (args) found.args = args;
    const plan = lineProblem(splitLine(draft.planArgs));
    if (plan) found.planArgs = plan;
    else if (draft.edits && !draft.planArgs.trim()) found.planArgs = "mods.planArgs.required";
  } else {
    if (!draft.baseUrl.trim()) found.baseUrl = "mods.baseUrl.required";
    else if (!/^https?:\/\/[^\s/@]+/i.test(draft.baseUrl.trim())) found.baseUrl = "gateway.baseUrl.invalid";
    if (draft.keyRequired && !draft.apiKey.trim()) found.apiKey = "gateway.apiKey.required";
    if (/\s/.test(draft.apiKey.trim())) found.apiKey = "gateway.apiKey.invalid";
  }
  if (draft.model.trim() && !/^[A-Za-z0-9._:/@[\]-]+$/.test(draft.model.trim())) found.model = "model.invalid";
  return found;
}

/** Criar mod: uma integração nova com modelos de LLM, de linha de comando
 * (um programa na máquina) ou de API (um endereço compatível com a OpenAI ou
 * com a Anthropic). O mod entra no rascunho das configurações, ligado, com a
 * aba dele aberta: vale depois do Salvar, como o resto da tela. */
export function ModCreateDialog({ children }: { children: ReactNode }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<ModDraft>(EMPTY_MOD);
  const [tried, setTried] = useState(false);
  const set = (patch: Partial<ModDraft>) => setDraft((current) => ({ ...current, ...patch }));
  const found = draftProblems(draft);
  const shown = (field: keyof ModDraft) => (tried && found[field] ? t(found[field]!) : undefined);
  const cli = draft.kind === "cli";

  const reset = (next: boolean) => {
    setOpen(next);
    if (next) { setDraft(EMPTY_MOD); setTried(false); }
  };
  useIntentHandler("createMod", () => reset(true));

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTried(true);
    if (Object.keys(found).length > 0) return;
    const id = createMod(draft);
    setOpen(false);
    setSettingsTab(id);
  };

  return (
    <Dialog open={open} onOpenChange={reset}>
      <DialogTrigger asChild>{children}</DialogTrigger>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-[560px]">
        <form onSubmit={submit} className="grid gap-5" noValidate>
          <DialogHeader>
            <Eyebrow>{t("settings.tab.mods")}</Eyebrow>
            <DialogTitle className="text-xl">{t("mods.create.title")}</DialogTitle>
            <DialogDescription>{t("mods.create.description")}</DialogDescription>
          </DialogHeader>
          <FormField label={t("mods.name")} htmlFor="mod-name" error={shown("name")} hint={t("mods.name.hint")}>
            <Input id="mod-name" autoFocus maxLength={40} value={draft.name} placeholder={t("mods.name.placeholder")} aria-invalid={shown("name") ? true : undefined} onChange={(event) => set({ name: event.target.value })} />
          </FormField>
          <div className="grid gap-1.5">
            <span className="text-xs text-muted-foreground">{t("mods.kind")}</span>
            <SegmentedControl
              label={t("mods.kind")}
              value={draft.kind}
              onChange={(kind) => set({ kind })}
              options={[
                { value: "cli", label: t("mods.kind.cli"), icon: <TerminalIcon className="size-3.5" />, hint: t("mods.kind.cli.hint") },
                { value: "api", label: t("mods.kind.api"), icon: <CloudIcon className="size-3.5" />, hint: t("mods.kind.api.hint") },
              ]}
            />
            <p className="text-xs leading-snug text-muted-foreground/80">{t(cli ? "mods.kind.cli.hint" : "mods.kind.api.hint")}</p>
          </div>
          {cli ? (
            <>
              <FormField label={t("mods.command")} htmlFor="mod-command" error={shown("command")} hint={t("mods.command.hint")}>
                <Input id="mod-command" spellCheck={false} className="font-mono" value={draft.command} placeholder="my-agent" aria-invalid={shown("command") ? true : undefined} onChange={(event) => set({ command: event.target.value })} />
              </FormField>
              <FormField label={t("mods.args")} htmlFor="mod-args" error={shown("args")} hint={t("mods.args.hint")}>
                <Input id="mod-args" spellCheck={false} className="font-mono text-xs" value={draft.args} placeholder="run --model {model} {prompt}" aria-invalid={shown("args") ? true : undefined} onChange={(event) => set({ args: event.target.value })} />
              </FormField>
              <ToggleRow id="mod-edits" label={t("mods.edits")} hint={t("mods.edits.hint")} checked={draft.edits} onChange={(edits) => set({ edits })} />
              <FormField label={t("mods.planArgs")} htmlFor="mod-plan-args" error={shown("planArgs")} hint={t(draft.edits ? "mods.planArgs.hint.required" : "mods.planArgs.hint")}>
                <Input id="mod-plan-args" spellCheck={false} className="font-mono text-xs" value={draft.planArgs} placeholder="run --read-only --model {model} {prompt}" aria-invalid={shown("planArgs") ? true : undefined} onChange={(event) => set({ planArgs: event.target.value })} />
              </FormField>
            </>
          ) : (
            <>
              <FormField label={t("mods.protocol")} htmlFor="mod-protocol" hint={t("mods.protocol.hint")}>
                <SegmentedControl
                  label={t("mods.protocol")}
                  value={draft.protocol}
                  onChange={(protocol) => set({ protocol })}
                  options={[{ value: "openai", label: t("mods.protocol.openai") }, { value: "anthropic", label: t("mods.protocol.anthropic") }]}
                />
              </FormField>
              <FormField label={t("gateway.baseUrl")} htmlFor="mod-url" error={shown("baseUrl")} hint={t("mods.baseUrl.hint")}>
                <Input id="mod-url" spellCheck={false} className="font-mono" value={draft.baseUrl} placeholder="http://localhost:8080/v1" aria-invalid={shown("baseUrl") ? true : undefined} onChange={(event) => set({ baseUrl: event.target.value })} />
              </FormField>
              <FormField label={t("gateway.apiKey")} htmlFor="mod-key" error={shown("apiKey")} hint={t(draft.keyRequired ? "gateway.apiKey.hint" : "mods.apiKey.hint")}>
                <Input id="mod-key" type="password" autoComplete="off" spellCheck={false} className="font-mono" value={draft.apiKey} placeholder={t("gateway.apiKey.paste")} aria-invalid={shown("apiKey") ? true : undefined} onChange={(event) => set({ apiKey: event.target.value })} />
              </FormField>
              <ToggleRow id="mod-key-required" label={t("mods.keyRequired")} hint={t("mods.keyRequired.hint")} checked={draft.keyRequired} onChange={(keyRequired) => set({ keyRequired })} />
            </>
          )}
          <FormField label={t("mods.model")} htmlFor="mod-model" error={shown("model")} hint={t(cli ? "mods.model.hint.cli" : "mods.model.hint.api")}>
            <Input id="mod-model" spellCheck={false} className="font-mono" value={draft.model} placeholder={t("model.custom.placeholder")} aria-invalid={shown("model") ? true : undefined} onChange={(event) => set({ model: event.target.value })} />
          </FormField>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setOpen(false)}>{t("common.cancel")}</Button>
            <Button type="submit">{t("mods.create")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
