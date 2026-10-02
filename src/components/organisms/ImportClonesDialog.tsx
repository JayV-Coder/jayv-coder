import { useState } from "react";
import type { FoundRepository } from "@/modules/core";
import { notify, reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { importClones } from "@/modules/organizations";
import { Eyebrow, PathText } from "@/components/atoms";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";

/** Os clones achados na pasta da organização: cada um marcado vira projeto.
 * Vem tudo marcado; a pessoa só desmarca o que não quer. */
export function ImportClonesDialog({ org, folder, clones, onClose }: { org: string; folder: string; clones: FoundRepository[] | null; onClose: () => void }) {
  const t = useT();
  const [skipped, setSkipped] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const chosen = (clones ?? []).filter((clone) => !skipped.has(clone.key));

  const toggle = (key: string, keep: boolean) => setSkipped((current) => {
    const next = new Set(current);
    if (keep) next.delete(key); else next.add(key);
    return next;
  });

  const close = () => { setSkipped(new Set()); onClose(); };

  const submit = async () => {
    setBusy(true);
    try {
      await importClones(chosen);
      notify(t("repos.import.done", { count: chosen.length }));
      close();
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={clones !== null} onOpenChange={(open) => { if (!open) close(); }}>
      <DialogContent className="sm:max-w-[520px]">
        <DialogHeader>
          <Eyebrow>{t("repos.folder.scan")}</Eyebrow>
          <DialogTitle className="text-xl">{t("repos.import.title")}</DialogTitle>
          <DialogDescription>{t("repos.import.description", { org, path: folder })}</DialogDescription>
        </DialogHeader>
        {clones?.length
          ? (
            <ul className="grid max-h-[50vh] gap-2 overflow-y-auto">
              {clones.map((clone) => (
                <li key={clone.key}>
                  <label className="flex cursor-pointer items-center gap-3 rounded-md border border-border/60 px-3 py-2.5 hover:bg-secondary">
                    <Checkbox checked={!skipped.has(clone.key)} onCheckedChange={(value) => toggle(clone.key, value === true)} />
                    <span className="grid min-w-0 flex-1">
                      <span className="truncate font-mono text-sm">{clone.key.split("/").slice(1).join("/")}</span>
                      <PathText title={clone.path} className="text-xs text-muted-foreground">{clone.path}</PathText>
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          )
          : <p className="text-sm text-muted-foreground">{t("repos.import.empty")}</p>}
        <DialogFooter>
          <Button type="button" variant="outline" onClick={close}>{t("common.cancel")}</Button>
          {Boolean(clones?.length) && (
            <Button type="button" disabled={busy || chosen.length === 0} aria-busy={busy || undefined} onClick={() => void submit()}>
              {t("repos.import.add", { count: chosen.length })}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
