import { useState, type FormEvent } from "react";
import { FolderSearchIcon, LoaderCircleIcon } from "lucide-react";
import type { FoundRepository } from "@/modules/core";
import { notify, reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import {
  addRepository, can, cloneRepository, forgetOrganizationFolder, linkFolder, localCopy, newClones, organizationFolder, parseRepoUrl, pickFolder,
  rememberOrganizationFolder, removeRepository, scanFolder, type OrganizationDetail, type Repository, type Role,
} from "@/modules/organizations";
import { openProject, useWorkspace } from "@/modules/workspace";
import { FolderIcon, PathText, PROVIDER_NAMES, ProviderIcon } from "@/components/atoms";
import { ConfirmAction, SettingsSection } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ImportClonesDialog } from "./ImportClonesDialog";

/** Os repositórios da organização e onde cada um está neste computador. Um
 * projeto local entra na organização quando um remote do git da pasta casa
 * com um deles. Quem não tem o repositório clona por aqui; quem já tem aponta
 * a pasta, um por um ou a pasta da organização inteira. */
export function OrganizationRepositories({ detail, role, name }: { detail: OrganizationDetail; role: Role; name: string }) {
  const t = useT();
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  // O repositório que está clonando agora: um por vez, para o erro de um não
  // se misturar com o de outro.
  const [cloning, setCloning] = useState<string | null>(null);
  const [folder, setFolder] = useState(() => organizationFolder(detail.id));
  const [found, setFound] = useState<FoundRepository[] | null>(null);
  const projects = useWorkspace((state) => state.data.projects);
  const parsed = parseRepoUrl(url);
  const manages = can.manage(role);

  const run = (action: () => Promise<void>) => {
    setBusy(true);
    return action().catch(reportError).finally(() => setBusy(false));
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!parsed) return;
    void run(async () => { await addRepository(detail.id, parsed.provider, parsed.path); setUrl(""); });
  };

  /** A pasta da organização: a guardada ou, na primeira vez, a que a pessoa
   * escolher agora. */
  const chooseFolder = async (ask: boolean) => {
    if (folder && !ask) return folder;
    const chosen = await pickFolder(t("repos.folder.pick", { org: name }), folder);
    if (chosen) { rememberOrganizationFolder(detail.id, chosen); setFolder(chosen); }
    return chosen;
  };

  const scan = (target: string) => run(async () => {
    const clones = await scanFolder(target, detail.repositories.map((repository) => repository.repoKey));
    setFound(newClones(clones, projects));
  });

  const changeFolder = () => run(async () => {
    const chosen = await chooseFolder(true);
    if (chosen && detail.repositories.length) await scan(chosen);
  });

  const clone = async (repository: Repository) => {
    try {
      const target = await chooseFolder(false);
      if (!target) return;
      setCloning(repository.id);
      const project = await cloneRepository(repository.repoKey, target);
      notify(t("repos.clone.done", { repo: repository.path, path: project.rootPath }));
    } catch (error) {
      reportError(error);
    } finally {
      setCloning(null);
    }
  };

  const link = (repository: Repository) => run(async () => {
    const chosen = await pickFolder(t("repos.link.pick", { repo: repository.path }), folder);
    if (!chosen) return;
    await linkFolder(repository.repoKey, chosen);
    notify(t("repos.link.done", { repo: repository.path }));
  });

  return (
    <SettingsSection title={t("org.repos.title")} description={t("org.repos.description")}>
      {manages && (
        <form onSubmit={submit} className="grid gap-2">
          <div className="flex flex-wrap gap-2">
            <Input aria-label={t("org.repos.url")} placeholder="https://github.com/acme/api" spellCheck={false} autoCapitalize="none"
              className="min-w-[240px] flex-1 font-mono text-sm" aria-invalid={url.trim() && !parsed ? true : undefined} value={url} onChange={(event) => setUrl(event.target.value)} />
            <Button type="submit" disabled={busy || !parsed}>{t("org.repos.add")}</Button>
          </div>
          <p className={url.trim() && !parsed ? "text-xs text-destructive" : "text-xs text-muted-foreground"}>
            {parsed
              ? <span className="inline-flex items-center gap-1.5"><ProviderIcon provider={parsed.provider} className="size-3.5" />{PROVIDER_NAMES[parsed.provider]} · <span className="font-mono">{parsed.path}</span></span>
              : url.trim() ? t("org.repos.invalid") : t("org.repos.hint")}
          </p>
        </form>
      )}
      <div className="flex flex-wrap items-center gap-3 rounded-md border border-border bg-secondary/40 px-3 py-3">
        <FolderIcon className="size-5 flex-none text-muted-foreground" />
        <div className="grid min-w-0 flex-1 gap-0.5">
          <span className="text-sm font-medium">{t("repos.folder.title")}</span>
          {folder
            ? <PathText title={folder} className="text-xs text-muted-foreground">{folder}</PathText>
            : <span className="text-xs text-muted-foreground">{t("repos.folder.none")}</span>}
        </div>
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" disabled={busy} onClick={() => void changeFolder()}>
            {folder ? t("repos.folder.change") : t("repos.folder.choose")}
          </Button>
          {folder && (
            <Button variant="outline" size="sm" disabled={busy || detail.repositories.length === 0} onClick={() => void scan(folder)}>
              <FolderSearchIcon />{t("repos.folder.scan")}
            </Button>
          )}
          {folder && (
            <ConfirmAction title={t("repos.folder.remove.title")} description={t("repos.folder.remove.description", { path: folder })}
              confirm={t("repos.folder.remove")} onConfirm={() => { forgetOrganizationFolder(detail.id); setFolder(null); }}>
              <Button variant="ghost" size="sm" disabled={busy || cloning !== null} className="text-muted-foreground hover:text-destructive">{t("repos.folder.remove")}</Button>
            </ConfirmAction>
          )}
        </div>
      </div>
      {detail.repositories.length === 0
        ? <p className="text-sm text-muted-foreground">{t("org.repos.empty")}</p>
        : (
          <ul className="grid gap-2">
            {detail.repositories.map((repository) => {
              const local = localCopy(repository.repoKey, projects);
              const working = cloning === repository.id;
              return (
                <li key={repository.id} className="flex flex-wrap items-center gap-3 rounded-md border border-border/60 px-3 py-2.5">
                  <ProviderIcon provider={repository.provider} className="size-5" />
                  <div className="grid min-w-0 flex-1 gap-0.5">
                    <p className="truncate font-mono text-sm">{repository.path}</p>
                    {local
                      ? (
                        <p className="flex min-w-0 items-center gap-1.5 text-xs">
                          <span aria-hidden="true" className="size-1.5 flex-none rounded-full bg-success" />
                          <span className="flex-none text-success">{t("repos.local")}</span>
                          <PathText title={local.rootPath} className="text-muted-foreground">{local.rootPath}</PathText>
                        </p>
                      )
                      : (
                        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
                          <span aria-hidden="true" className="size-1.5 flex-none rounded-full bg-muted-foreground/50" />
                          {PROVIDER_NAMES[repository.provider]} · {t("repos.remote")}
                        </p>
                      )}
                  </div>
                  <div className="flex flex-wrap items-center gap-1.5">
                    {local
                      ? <Button variant="outline" size="sm" onClick={() => openProject(local.id)}>{t("repos.open")}</Button>
                      : (
                        <>
                          <Button size="sm" disabled={busy || cloning !== null} aria-busy={working || undefined} onClick={() => void clone(repository)}>
                            {working && <LoaderCircleIcon className="animate-spin" />}
                            {working ? t("repos.cloning") : t("repos.clone")}
                          </Button>
                          <Button variant="ghost" size="sm" disabled={busy || cloning !== null} onClick={() => void link(repository)}>{t("repos.link")}</Button>
                        </>
                      )}
                    {manages && (
                      <ConfirmAction title={t("org.repos.remove.title")} description={t("org.repos.remove.description", { repo: repository.repoKey })}
                        confirm={t("org.repos.remove")} onConfirm={() => void run(() => removeRepository(repository.id))}>
                        <Button variant="ghost" size="sm" disabled={busy} className="text-muted-foreground hover:text-destructive">{t("org.repos.remove")}</Button>
                      </ConfirmAction>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      <ImportClonesDialog org={name} folder={folder ?? ""} clones={found} onClose={() => setFound(null)} />
    </SettingsSection>
  );
}
