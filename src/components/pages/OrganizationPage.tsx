import { ChartColumnIcon, DoorOpenIcon, FolderGit2Icon, FolderKanbanIcon, UsersIcon } from "lucide-react";
import { useT, type Key } from "@/modules/i18n";
import { navigate } from "@/modules/navigation";
import { setOrganizationTab, useOrganizations, type OrganizationTab } from "@/modules/organizations";
import { BackMark, LoadingNote } from "@/components/atoms";
import { PageHeading } from "@/components/molecules";
import { OrganizationChatButton, OrganizationGate, OrganizationMembers, OrganizationProjects, OrganizationRepositories, OrganizationStats, SiteDashboardButton } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

/** Uma organização: os projetos de quem usa o app que entraram nela, as
 * estatísticas e a portaria deles, membros e repositórios. Convidar, a
 * política de LLM e as configurações (renomear, sair, excluir) moram no
 * painel do site. Quem é member vê membros e repositórios só para leitura. */
export function OrganizationPage() {
  const t = useT();
  const openId = useOrganizations((state) => state.openId);
  const organization = useOrganizations((state) => state.list.find((org) => org.id === state.openId));
  const detail = useOrganizations((state) => (state.detail?.id === openId ? state.detail : null));
  const current = useOrganizations((state) => state.tab);

  if (!organization) {
    return (
      <ScrollPage>
        <Button variant="ghost" onClick={() => navigate("organizations")}><BackMark /> {t("org.back")}</Button>
      </ScrollPage>
    );
  }

  const tab = (value: string, Icon: typeof UsersIcon, key: Key) => (
    <TabsTrigger value={value} className="flex-none gap-2">
      <Icon className="size-4" />
      <span>{t(key)}</span>
    </TabsTrigger>
  );

  return (
    <ScrollPage>
      <PageHeading back={{ label: t("org.back"), onClick: () => navigate("organizations") }} title={organization.name}
        description={<span className="flex items-center gap-2"><span className="font-mono">@{organization.slug}</span><Badge variant="outline">{t(`org.role.${organization.role}` as Key)}</Badge></span>}>
        <OrganizationChatButton organization={organization} repositories={detail?.repositories ?? null} />
        <SiteDashboardButton path={`/organizations/${organization.id}`} />
      </PageHeading>
      <Tabs value={current} onValueChange={(value) => setOrganizationTab(value as OrganizationTab)} className="gap-5">
        <TabsList className="h-auto w-full flex-wrap justify-start">
          {tab("projects", FolderKanbanIcon, "org.tab.projects")}
          {tab("stats", ChartColumnIcon, "org.tab.stats")}
          {tab("gate", DoorOpenIcon, "org.tab.gate")}
          {tab("members", UsersIcon, "org.tab.members")}
          {tab("repositories", FolderGit2Icon, "org.tab.repositories")}
        </TabsList>
        <TabsContent value="projects"><OrganizationProjects orgId={organization.id} /></TabsContent>
        <TabsContent value="stats"><OrganizationStats orgId={organization.id} /></TabsContent>
        <TabsContent value="gate"><OrganizationGate orgId={organization.id} /></TabsContent>
        <TabsContent value="members">
          {detail ? <OrganizationMembers detail={detail} role={organization.role} /> : <LoadingNote>{t("settings.loading")}</LoadingNote>}
        </TabsContent>
        <TabsContent value="repositories">
          {detail ? <OrganizationRepositories detail={detail} role={organization.role} name={organization.name} /> : <LoadingNote>{t("settings.loading")}</LoadingNote>}
        </TabsContent>
      </Tabs>
    </ScrollPage>
  );
}
