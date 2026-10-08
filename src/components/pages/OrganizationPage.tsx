import { ChartColumnIcon, DoorOpenIcon, FolderGit2Icon, FolderKanbanIcon, PlugIcon, SparklesIcon, UsersIcon } from "lucide-react";
import { allows, useEntitlements } from "@/modules/plans";
import { useT, type Key } from "@/modules/i18n";
import { navigate } from "@/modules/navigation";
import { useEnvironment } from "@/modules/environments";
import { setOrganizationTab, useOrganizations, type OrganizationTab } from "@/modules/organizations";
import { BackMark, LoadingNote } from "@/components/atoms";
import { PageHeading } from "@/components/molecules";
import { EnvironmentGate, FeatureLocked, OrganizationChatButton, OrganizationExtensions, OrganizationGate, OrganizationMembers, OrganizationProjects, OrganizationRepositories, OrganizationStats, SiteDashboardButton } from "@/components/organisms";
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
  const rights = useEntitlements();
  const environment = useEnvironment((state) => state.active);

  if (!organization) {
    return (
      <ScrollPage>
        <Button variant="ghost" onClick={() => navigate("organizations")}><BackMark /> {t("org.back")}</Button>
      </ScrollPage>
    );
  }

  const tab = (value: string, Icon: typeof UsersIcon, key: Key) => (
    <TabsTrigger value={value} className="flex-none gap-2.5 px-3 py-2">
      <Icon className="size-5" />
      <span>{t(key)}</span>
    </TabsTrigger>
  );

  return (
    <ScrollPage>
      <PageHeading back={{ label: t("org.back"), onClick: () => navigate("organizations") }} title={organization.name}
        description={<span className="flex items-center gap-2"><span className="font-mono">@{organization.slug}</span><Badge variant="outline">{t(`org.role.${organization.role}` as Key)}</Badge></span>}>
        {environment === organization.id && <OrganizationChatButton organization={organization} repositories={detail?.repositories ?? null} />}
        <SiteDashboardButton path={`/organizations/${organization.id}`} />
      </PageHeading>
      <Tabs value={current} onValueChange={(value) => setOrganizationTab(value as OrganizationTab)} orientation="vertical" className="gap-6">
        {/* Na vertical, como no site e em Configurações. */}
        <TabsList className="sticky top-0 h-auto w-52 shrink-0 gap-0.5 py-1 pr-1">
          {tab("projects", FolderKanbanIcon, "org.tab.projects")}
          {tab("stats", ChartColumnIcon, "org.tab.stats")}
          {tab("gate", DoorOpenIcon, "org.tab.gate")}
          {tab("members", UsersIcon, "org.tab.members")}
          {tab("repositories", FolderGit2Icon, "org.tab.repositories")}
          {tab("mcp", PlugIcon, "settings.tab.mcp")}
          {tab("skills", SparklesIcon, "settings.tab.skills")}
        </TabsList>
        <TabsContent value="projects" className="min-w-0"><EnvironmentGate organization={organization}><OrganizationProjects /></EnvironmentGate></TabsContent>
        <TabsContent value="stats" className="min-w-0"><EnvironmentGate organization={organization}><OrganizationStats orgId={organization.id} /></EnvironmentGate></TabsContent>
        <TabsContent value="gate" className="min-w-0"><EnvironmentGate organization={organization}><OrganizationGate orgId={organization.id} /></EnvironmentGate></TabsContent>
        <TabsContent value="members" className="min-w-0">
          {detail ? <OrganizationMembers detail={detail} role={organization.role} /> : <LoadingNote>{t("settings.loading")}</LoadingNote>}
        </TabsContent>
        <TabsContent value="repositories" className="min-w-0">
          {detail
            ? <EnvironmentGate organization={organization}><OrganizationRepositories detail={detail} role={organization.role} name={organization.name} /></EnvironmentGate>
            : <LoadingNote>{t("settings.loading")}</LoadingNote>}
        </TabsContent>
        <TabsContent value="mcp" className="min-w-0">{allows(rights, "mcp") ? <OrganizationExtensions organization={organization} kind="mcp" /> : <FeatureLocked feature="mcp" />}</TabsContent>
        <TabsContent value="skills" className="min-w-0">{allows(rights, "skills") ? <OrganizationExtensions organization={organization} kind="skills" /> : <FeatureLocked feature="skills" />}</TabsContent>
      </Tabs>
    </ScrollPage>
  );
}
