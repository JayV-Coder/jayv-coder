import { FolderGit2Icon, FolderKanbanIcon, SettingsIcon, ShieldCheckIcon, UsersIcon } from "lucide-react";
import { useT, type Key } from "@/modules/i18n";
import { navigate } from "@/modules/navigation";
import { setOrganizationTab, useOrganizations, type OrganizationTab } from "@/modules/organizations";
import { LoadingNote } from "@/components/atoms";
import { PageHeading } from "@/components/molecules";
import { OrganizationMembers, OrganizationPolicy, OrganizationProjects, OrganizationRepositories, OrganizationSettings } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

/** Uma organização: os projetos de quem usa o app que entraram nela, membros,
 * repositórios, política de LLM e configurações. Quem é member vê membros,
 * repositórios e política só para leitura. */
export function OrganizationPage() {
  const t = useT();
  const openId = useOrganizations((state) => state.openId);
  const organization = useOrganizations((state) => state.list.find((org) => org.id === state.openId));
  const detail = useOrganizations((state) => (state.detail?.id === openId ? state.detail : null));
  const current = useOrganizations((state) => state.tab);

  if (!organization) {
    return (
      <ScrollPage>
        <Button variant="ghost" onClick={() => navigate("organizations")}>← {t("nav.organizations")}</Button>
      </ScrollPage>
    );
  }

  const tab = (value: string, Icon: typeof UsersIcon, key: Key) => (
    <TabsTrigger value={value} className="flex-none gap-2.5 px-4 py-2">
      <Icon className="size-4" />
      <span>{t(key)}</span>
    </TabsTrigger>
  );

  return (
    <ScrollPage>
      <PageHeading eyebrow={t("nav.organizations")} title={organization.name}
        description={<span className="flex items-center gap-2"><span className="font-mono">@{organization.slug}</span><Badge variant="outline">{t(`org.role.${organization.role}` as Key)}</Badge></span>}>
        <Button variant="outline" onClick={() => navigate("organizations")}>{t("org.back")}</Button>
      </PageHeading>
      <Tabs value={current} onValueChange={(value) => setOrganizationTab(value as OrganizationTab)} className="gap-5">
        <TabsList className="h-auto w-full justify-start gap-1 overflow-x-auto p-1">
          {tab("projects", FolderKanbanIcon, "org.tab.projects")}
          {tab("members", UsersIcon, "org.tab.members")}
          {tab("repositories", FolderGit2Icon, "org.tab.repositories")}
          {tab("policy", ShieldCheckIcon, "org.tab.policy")}
          {tab("settings", SettingsIcon, "org.tab.settings")}
        </TabsList>
        <TabsContent value="projects"><OrganizationProjects orgId={organization.id} /></TabsContent>
        <TabsContent value="members">
          {detail ? <OrganizationMembers detail={detail} role={organization.role} /> : <LoadingNote>{t("settings.loading")}</LoadingNote>}
        </TabsContent>
        <TabsContent value="repositories">
          {detail ? <OrganizationRepositories detail={detail} role={organization.role} name={organization.name} /> : <LoadingNote>{t("settings.loading")}</LoadingNote>}
        </TabsContent>
        <TabsContent value="policy">
          {detail ? <OrganizationPolicy detail={detail} role={organization.role} /> : <LoadingNote>{t("settings.loading")}</LoadingNote>}
        </TabsContent>
        <TabsContent value="settings"><OrganizationSettings key={organization.id + organization.name} organization={organization} /></TabsContent>
      </Tabs>
    </ScrollPage>
  );
}
