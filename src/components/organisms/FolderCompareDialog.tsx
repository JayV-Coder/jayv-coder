import { useEffect, useState } from "react";
import { CheckIcon, LoaderCircleIcon } from "lucide-react";
import { notify, reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { cloneRepository, importClones, type FolderComparison, type Repository } from "@/modules/organizations";
import { Eyebrow, PathText, ProviderIcon } from "@/components/atoms";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";

export interface FolderReport { comparison: FolderComparison<Repository>; truncated: boolean }

/** "Procurar clones": a pasta da organização, olhada em qualquer
 * profundidade, comparada com os repositórios que o owner associou no site.
 * Os clones que ainda não são projeto vêm marcados para virar projeto; os
 * repositórios que faltam clonam um a um ou todos de uma vez; e os clones da
 * pasta que a organização não tem aparecem à parte. Cada clone novo refaz a
 * comparação (`onChanged`). */
export function FolderCompareDialog({ org, folder, report, onChanged, onClose }: {
  org: string;
  folder: string;
  report: FolderReport | null;
  onChanged: () => Promise<void>;
  onClose: () => void;
}) {
  const t = useT();
  const [skipped, setSkipped] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [cloning, setCloning] = useState<string | null>(null);
  const comparison = report?.comparison;
  const fresh = (comparison?.found ?? []).filter((item) => item.state === "new");
  const chosen = fresh.filter((item) => !skipped.has(item.path));
  const total = comparison ? comparison.found.length + comparison.missing.length : 0;

  // Uma comparação nova (outra busca) começa com tudo marcado de novo.
  useEffect(() => setSkipped(new Set()), [report]);

  const toggle = (path: string, keep: boolean) => setSkipped((current) => {
    const next = new Set(current);
    if (keep) next.delete(path); else next.add(path);
    return next;
  });

  const close = () => {
    if (busy) return;
    onClose();
  };

  const importChosen = async () => {
    setBusy(true);
    try {
      await importClones(chosen.map((item) => item.path));
      notify(t("repos.import.done", { count: chosen.length }));
      await onChanged();
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  /** Clona na pasta da organização, um por vez; o erro de um não para os
   * outros. */
  const clone = async (repositories: Repository[]) => {
    setBusy(true);
    let cloned = 0;
    for (const repository of repositories) {
      setCloning(repository.id);
      try {
        const project = await cloneRepository(repository.repoKey, folder);
        cloned += 1;
        notify(t("repos.clone.done", { repo: repository.path, path: project.rootPath }));
      } catch (error) {
        reportError(error);
      }
    }
    setCloning(null);
    try {
      if (cloned > 0) await onChanged();
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={report !== null} onOpenChange={(open) => { if (!open) close(); }}>
      <DialogContent className="grid-cols-[minmax(0,1fr)] sm:max-w-[600px]">
        <DialogHeader>
          <Eyebrow>{t("repos.folder.scan")}</Eyebrow>
          <DialogTitle className="text-xl">{t("repos.compare.title")}</DialogTitle>
          <DialogDescription>
            {t("repos.compare.description", { org, path: folder, found: comparison?.found.length ?? 0, total })}
          </DialogDescription>
        </DialogHeader>

        {comparison && (
          <div className="grid max-h-[60vh] grid-cols-[minmax(0,1fr)] gap-5 overflow-y-auto pe-1">
            {report?.truncated && <p className="rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs text-warning">{t("repos.compare.truncated")}</p>}
            {total === 0 && comparison.outside.length === 0 && <p className="text-sm text-muted-foreground">{t("repos.compare.empty")}</p>}
            {total > 0 && comparison.missing.length === 0 && fresh.length === 0 && (
              <p className="flex items-center gap-2 text-sm text-success"><CheckIcon className="size-4" />{t("repos.compare.allSet")}</p>
            )}

            {comparison.found.length > 0 && (
              <section className="grid grid-cols-[minmax(0,1fr)] gap-2">
                <h3 className="text-sm font-semibold">{t("repos.compare.found")} <span className="text-muted-foreground">({comparison.found.length})</span></h3>
                <ul className="grid grid-cols-[minmax(0,1fr)] gap-2">
                  {comparison.found.map((item) => (
                    <li key={item.repository.id}>
                      <label className={`flex items-center gap-3 rounded-md border border-border/60 px-3 py-2.5 ${item.state === "new" ? "cursor-pointer hover:bg-secondary" : ""}`}>
                        {item.state === "new"
                          ? <Checkbox checked={!skipped.has(item.path)} disabled={busy} onCheckedChange={(value) => toggle(item.path, value === true)} />
                          : <ProviderIcon provider={item.repository.provider} className="size-4 flex-none" />}
                        <span className="grid min-w-0 flex-1 grid-cols-[minmax(0,1fr)]">
                          <span className="truncate font-mono text-sm">{item.repository.path}</span>
                          <PathText title={item.path} className="text-xs text-muted-foreground">{item.path}</PathText>
                          {item.elsewhere && <span className="truncate text-xs text-warning" title={item.elsewhere}>{t("repos.compare.elsewhere", { path: item.elsewhere })}</span>}
                        </span>
                        {item.state === "project" && <Badge variant="success">{t("repos.compare.project")}</Badge>}
                      </label>
                    </li>
                  ))}
                </ul>
              </section>
            )}

            {comparison.missing.length > 0 && (
              <section className="grid grid-cols-[minmax(0,1fr)] gap-2">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <h3 className="text-sm font-semibold">{t("repos.compare.missing")} <span className="text-muted-foreground">({comparison.missing.length})</span></h3>
                  {comparison.missing.length > 1 && (
                    <Button size="sm" variant="outline" disabled={busy} onClick={() => void clone(comparison.missing.map((item) => item.repository))}>{t("repos.compare.cloneAll")}</Button>
                  )}
                </div>
                <ul className="grid grid-cols-[minmax(0,1fr)] gap-2">
                  {comparison.missing.map(({ repository, local }) => {
                    const working = cloning === repository.id;
                    return (
                      <li key={repository.id} className="flex items-center gap-3 rounded-md border border-dashed border-border/70 px-3 py-2.5">
                        <ProviderIcon provider={repository.provider} className="size-4 flex-none" />
                        <span className="grid min-w-0 flex-1 grid-cols-[minmax(0,1fr)]">
                          <span className="truncate font-mono text-sm">{repository.path}</span>
                          {local && <PathText title={local} className="text-xs text-muted-foreground">{t("repos.compare.localElsewhere", { path: local })}</PathText>}
                        </span>
                        <Button size="sm" disabled={busy} aria-busy={working || undefined} onClick={() => void clone([repository])}>
                          {working && <LoaderCircleIcon className="animate-spin" />}
                          {working ? t("repos.cloning") : t("repos.clone")}
                        </Button>
                      </li>
                    );
                  })}
                </ul>
              </section>
            )}

            {comparison.outside.length > 0 && (
              <section className="grid grid-cols-[minmax(0,1fr)] gap-2">
                <div>
                  <h3 className="text-sm font-semibold">{t("repos.compare.outside")} <span className="text-muted-foreground">({comparison.outside.length})</span></h3>
                  <p className="text-xs text-muted-foreground">{t("repos.compare.outside.hint")}</p>
                </div>
                <ul className="grid grid-cols-[minmax(0,1fr)] gap-1.5">
                  {comparison.outside.map((clone) => (
                    <li key={clone.path} className="grid grid-cols-[minmax(0,1fr)] rounded-md bg-secondary/50 px-3 py-2">
                      <PathText title={clone.path} className="text-xs">{clone.path}</PathText>
                      <span className="truncate font-mono text-xs text-muted-foreground">{clone.keys[0] ?? t("repos.compare.noRemote")}</span>
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </div>
        )}

        <DialogFooter>
          <Button type="button" variant="outline" disabled={busy} onClick={close}>{t("common.close")}</Button>
          {fresh.length > 0 && (
            <Button type="button" disabled={busy || chosen.length === 0} aria-busy={busy || undefined} onClick={() => void importChosen()}>
              {t("repos.import.add", { count: chosen.length })}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
