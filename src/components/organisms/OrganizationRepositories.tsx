import { useState } from "react";
import { ExternalLinkIcon, FolderSearchIcon, GitBranchIcon, GlobeIcon, LoaderCircleIcon, LockIcon } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { notify, reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import {
  cloneRepository, compareFolder, forgetOrganizationFolder, linkFolder, localCopy, organizationFolder, pickFolder,
  rememberOrganizationFolder, scanFolder, type OrganizationDetail, type Repository, type Role,
} from "@/modules/organizations";
import { openProject, useWorkspace } from "@/modules/workspace";
import { FolderIcon, PathText, PROVIDER_NAMES, ProviderIcon } from "@/components/atoms";
import { ConfirmAction, SettingsSection } from "@/components/molecules";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { FolderCompareDialog, type FolderReport } from "./FolderCompareDialog";
import { SiteDashboardButton } from "./SiteDashboardButton";

/** Os repositórios da organização e onde cada um está neste computador. Quem
 * escolhe os repositórios é o owner, no site, pelo GitHub, GitLab ou
 * Bitbucket; o app recebe a lista ao vivo e cada membro clona com o próprio
 * acesso git. Um projeto local entra na organização quando um remote do git
 * da pasta casa com um deles. "Procurar clones" olha a pasta da organização
 * inteira e compara com a lista do site. */
export function OrganizationRepositories({ detail, role, name }: { detail: OrganizationDetail; role: Role; name: string }) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  // O repositório que está clonando agora: um por vez, para o erro de um não
  // se misturar com o de outro.
  const [cloning, setCloning] = useState<string | null>(null);
  const [folder, setFolder] = useState(() => organizationFolder(detail.id));
  const [report, setReport] = useState<FolderReport | null>(null);
  const projects = useWorkspace((state) => state.data.projects);

  const run = (action: () => Promise<void>) => {
    setBusy(true);
    return action().catch(reportError).finally(() => setBusy(false));
  };

  /** A pasta da organização: a guardada ou, na primeira vez, a que a pessoa
   * escolher agora. */
  const chooseFolder = async (ask: boolean) => {
    if (folder && !ask) return folder;
    const chosen = await pickFolder(t("repos.folder.pick", { org: name }), folder);
    if (chosen) { rememberOrganizationFolder(detail.id, chosen); setFolder(chosen); }
    return chosen;
  };

  /** Busca os clones na pasta e compara com a lista do site. Os projetos são
   * lidos na hora: um clone ou uma importação acabou de mudá-los. */
  const compare = async (target: string) => {
    const scan = await scanFolder(target);
    const current = useWorkspace.getState().data.projects;
    setReport({ comparison: compareFolder(detail.repositories, scan.clones, current), truncated: scan.truncated });
  };

  const scan = (target: string) => run(() => compare(target));

  const changeFolder = () => run(async () => {
    const chosen = await chooseFolder(true);
    if (chosen) await compare(chosen);
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
      <div className="flex flex-wrap items-center gap-x-4 gap-y-3 rounded-md border border-border bg-secondary/40 px-3 py-3">
        <div className="grid min-w-[min(100%,16rem)] flex-1 gap-1.5">
          <span className="text-sm">{t(role === "owner" ? "repos.site.owner" : "repos.site.member")}</span>
          {detail.connections.length > 0
            ? (
              <span className="flex flex-wrap gap-1.5">
                {detail.connections.map((connection) => (
                  <Badge key={connection.provider} variant="outline" className="max-w-full">
                    <ProviderIcon provider={connection.provider} />
                    <span className="truncate">{PROVIDER_NAMES[connection.provider]} · {connection.account}</span>
                  </Badge>
                ))}
              </span>
            )
            : <span className="text-xs text-muted-foreground">{t("repos.site.none")}</span>}
        </div>
        <SiteDashboardButton path={`/organizations/${detail.id}?tab=repositories`} size="sm" />
      </div>
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
            <Button variant="outline" size="sm" disabled={busy || cloning !== null} aria-busy={busy || undefined} onClick={() => void scan(folder)}>
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
          <ul className="grid grid-cols-[minmax(0,1fr)] gap-2">
            {detail.repositories.map((repository) => {
              const local = localCopy(repository.repoKey, projects);
              const working = cloning === repository.id;
              return (
                <li key={repository.id} className="flex flex-wrap items-center gap-3 rounded-md border border-border/60 px-3 py-2.5">
                  <ProviderIcon provider={repository.provider} className="size-5" />
                  <div className="grid min-w-[min(100%,14rem)] flex-1 grid-cols-[minmax(0,1fr)] gap-0.5">
                    <p className="flex min-w-0 items-center gap-2">
                      <span className="truncate font-mono text-sm" title={repository.repoKey}>{repository.path}</span>
                      {repository.private !== null && (
                        <Badge variant="outline" className="flex-none">{repository.private ? <LockIcon /> : <GlobeIcon />}{t(repository.private ? "repos.private" : "repos.public")}</Badge>
                      )}
                      {repository.defaultBranch && <Badge variant="secondary" className="hidden min-w-0 sm:inline-flex"><GitBranchIcon /><span className="truncate">{repository.defaultBranch}</span></Badge>}
                    </p>
                    {repository.description && <p className="truncate text-xs text-muted-foreground" title={repository.description}>{repository.description}</p>}
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
                    {repository.webUrl && (
                      <Button variant="ghost" size="icon-sm" aria-label={t("repos.openWeb", { provider: PROVIDER_NAMES[repository.provider] })}
                        title={t("repos.openWeb", { provider: PROVIDER_NAMES[repository.provider] })} onClick={() => void openUrl(repository.webUrl!).catch(reportError)}>
                        <ExternalLinkIcon />
                      </Button>
                    )}
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
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      <FolderCompareDialog org={name} folder={folder ?? ""} report={report}
        onChanged={() => (folder ? compare(folder) : Promise.resolve())} onClose={() => setReport(null)} />
    </SettingsSection>
  );
}
