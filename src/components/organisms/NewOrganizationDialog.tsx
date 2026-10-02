import { useState, type FormEvent, type ReactNode } from "react";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { createOrganization, SLUG_MAX, slugify, slugOk } from "@/modules/organizations";
import { FormField } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

/** A organização nova: o nome sugere o slug até alguém mexer nele. Quem cria
 * vira owner. */
export function NewOrganizationDialog({ children }: { children: ReactNode }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [typed, setTyped] = useState(false);
  const [busy, setBusy] = useState(false);
  const badSlug = slug.length > 0 && !slugOk(slug);

  const reset = (next: boolean) => {
    setOpen(next);
    if (next) { setName(""); setSlug(""); setTyped(false); }
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim() || !slugOk(slug)) return;
    setBusy(true);
    try {
      await createOrganization(name, slug);
      setOpen(false);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={reset}>
      <DialogTrigger asChild>{children}</DialogTrigger>
      <DialogContent className="sm:max-w-[480px]">
        <form onSubmit={submit} className="grid gap-5">
          <DialogHeader>
            <DialogTitle>{t("org.new.title")}</DialogTitle>
            <DialogDescription>{t("org.new.description")}</DialogDescription>
          </DialogHeader>
          <FormField label={t("org.field.name")} htmlFor="org-name">
            <Input id="org-name" required maxLength={80} value={name} autoFocus
              onChange={(event) => { setName(event.target.value); if (!typed) setSlug(slugify(event.target.value)); }} />
          </FormField>
          <FormField label={t("org.field.slug")} htmlFor="org-slug" hint={t("org.field.slug.hint")} error={badSlug ? t("org.field.slug.invalid", { max: SLUG_MAX }) : null}>
            <div className="relative">
              <span aria-hidden="true" className="pointer-events-none absolute inset-y-0 start-3 grid place-items-center text-sm text-muted-foreground">@</span>
              <Input id="org-slug" required maxLength={SLUG_MAX} spellCheck={false} autoCapitalize="none" className="ps-7" aria-invalid={badSlug || undefined} value={slug}
                onChange={(event) => { setTyped(true); setSlug(event.target.value.toLowerCase().replace(/\s/g, "-")); }} />
            </div>
          </FormField>
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => setOpen(false)}>{t("common.cancel")}</Button>
            <Button type="submit" disabled={busy || !name.trim() || !slugOk(slug)}>{t("org.new.create")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
