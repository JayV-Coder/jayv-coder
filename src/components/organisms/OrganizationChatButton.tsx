import { useState, type ComponentProps } from "react";
import { FolderTreeIcon } from "lucide-react";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import {
  chatReach, openOrganizationChat, organizationFolder, organizationRepositories, pickFolder, rememberOrganizationFolder, resumeOrganizationChat,
  type Repository,
} from "@/modules/organizations";
import { useFeature } from "@/modules/plans";
import { useWorkspace } from "@/modules/workspace";
import { Eyebrow, PathText } from "@/components/atoms";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";

/** O chat em todos os repositórios da organização: um chat com a pasta da
 * organização como raiz, que enxerga todos os clones dela de uma vez. Quando
 * esse chat já existe neste computador, o botão leva direto a ele; senão,
 * antes de abrir, mostra o que entra e o que fica de fora. Na página da
 * organização os repositórios já vêm carregados; na lista de projetos, são
 * buscados no clique. */
export function OrganizationChatButton({ organization, repositories, size, variant }: {
  organization: { id: string; name: string };
  repositories?: Repository[] | null;
  size?: ComponentProps<typeof Button>["size"];
  variant?: ComponentProps<typeof Button>["variant"];
}) {
  const t = useT();
  const allowed = useFeature("orgChat");
  const [folder, setFolder] = useState<string | null>(null);
  const [fetched, setFetched] = useState<Repository[] | null>(null);
  const [busy, setBusy] = useState(false);
  const projects = useWorkspace((state) => state.data.projects);
  const loaded = repositories ?? fetched;
  const reach = folder && loaded ? chatReach(loaded, projects, folder) : null;

  const open = async () => {
    if (resumeOrganizationChat(organization.id)) return;
    if (!repositories) setFetched(await organizationRepositories(organization.id));
    await choose(false);
  };

  const choose = async (ask: boolean) => {
    const known = organizationFolder(organization.id);
    if (known && !ask) { setFolder(known); return; }
    const chosen = await pickFolder(t("repos.folder.pick", { org: organization.name }), known);
    if (!chosen) return;
    rememberOrganizationFolder(organization.id, chosen);
    setFolder(chosen);
  };

  const start = async () => {
    if (!folder) return;
    setBusy(true);
    try {
      await openOrganizationChat(organization.id, organization.name, folder);
      setFolder(null);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  const list = (title: string, items: { key: string; label: string; path?: string }[]) => items.length > 0 && (
    <section className="grid gap-2">
      <Eyebrow>{title}</Eyebrow>
      <ul className="grid gap-1.5">
        {items.map((item) => (
          <li key={item.key} className="grid min-w-0 rounded-md border border-border/60 px-3 py-2">
            <span className="truncate font-mono text-sm">{item.label}</span>
            {item.path && <PathText title={item.path} className="text-xs text-muted-foreground">{item.path}</PathText>}
          </li>
        ))}
      </ul>
    </section>
  );

  // Fora do plano (ou desligado pelo admin), o botão some.
  if (!allowed) return null;
  return (
    <>
      <Button size={size} variant={variant} disabled={repositories === null} onClick={() => void open().catch(reportError)}>
        <FolderTreeIcon className="size-4" />
        {t("orgChat.open")}
      </Button>
      <Dialog open={folder !== null} onOpenChange={(open) => { if (!open) setFolder(null); }}>
        <DialogContent className="sm:max-w-[560px]">
          <DialogHeader>
            <Eyebrow>{organization.name}</Eyebrow>
            <DialogTitle className="text-xl">{t("orgChat.title")}</DialogTitle>
            <DialogDescription>{t("orgChat.description", { org: organization.name })}</DialogDescription>
          </DialogHeader>
          {folder && reach && (
            <div className="grid max-h-[55vh] gap-4 overflow-y-auto">
              <section className="flex items-center justify-between gap-3 rounded-md border border-border/60 px-3 py-2.5">
                <span className="grid min-w-0">
                  <span className="text-xs text-muted-foreground">{t("repos.folder.title")}</span>
                  <PathText title={folder} className="text-sm">{folder}</PathText>
                </span>
                <Button type="button" variant="outline" size="sm" onClick={() => void choose(true).catch(reportError)}>{t("repos.folder.change")}</Button>
              </section>
              {reach.inside.length === 0 && <p className="text-sm text-muted-foreground">{t("orgChat.none")}</p>}
              {list(t("orgChat.inside"), reach.inside.map((item) => ({ key: item.repository.id, label: item.repository.path, path: item.path })))}
              {list(t("orgChat.elsewhere"), reach.elsewhere.map((item) => ({ key: item.repository.id, label: item.repository.path, path: item.path })))}
              {list(t("orgChat.missing"), reach.missing.map((repository) => ({ key: repository.id, label: repository.path })))}
            </div>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setFolder(null)}>{t("common.cancel")}</Button>
            <Button type="button" disabled={busy} aria-busy={busy || undefined} onClick={() => void start()}>{t("orgChat.start")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
