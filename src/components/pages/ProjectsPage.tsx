import { useEffect, useMemo, useState } from "react";
import { SearchIcon } from "lucide-react";
import { loadOrganizations, searchScopeGroups, splitGeneral, useOrganizations, type ScopeGroup } from "@/modules/organizations";
import { PERSONAL, useEnvironment } from "@/modules/environments";
import type { Project } from "@/modules/core";
import { setLayout, useWorkspace } from "@/modules/workspace";
import { useT, type Key } from "@/modules/i18n";
import { EmptyText } from "@/components/atoms";
import { LayoutSwitch, PageHeading } from "@/components/molecules";
import { NewProjectDialog, OrganizationChatButton, ProjectScopeSection } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** Os projetos do ambiente aberto: o banco de cada ambiente só tem os dele, então
 * a lista é um bloco só. No pessoal, o título é Projetos e o bloco diz que são
 * pessoais; no ambiente de uma organização, o título da página é ela (nome,
 * @slug, quantos projetos, o papel e de onde eles vêm) e, no lugar de "Novo
 * projeto", fica o chat dela — os projetos da organização chegam pelos
 * repositórios dela. A busca procura pelo nome, pela pasta e pelos
 * repositórios. */
export function ProjectsPage() {
  const t = useT();
  const { data, layout } = useWorkspace();
  const organizations = useOrganizations((state) => state.list);
  const environment = useEnvironment((state) => state.active);
  const [query, setQuery] = useState("");
  useEffect(() => { loadOrganizations().catch((error) => console.error("organizations", error)); }, []);

  const group = useMemo((): ScopeGroup<Project> => {
    const organization = environment === PERSONAL ? undefined : organizations.find((item) => item.id === environment);
    return organization
      ? { key: organization.id, scope: { kind: "organization", orgId: organization.id }, name: organization.name, slug: organization.slug, role: organization.role, projects: data.projects }
      : { key: "personal", scope: { kind: "personal" }, name: "", slug: null, role: null, projects: data.projects };
  }, [environment, organizations, data.projects]);
  const shown = searchScopeGroups([group], query);
  // O ambiente é da organização mesmo enquanto a lista dela ainda não chegou.
  const inOrganization = environment !== PERSONAL;
  const organization = group.scope.kind === "organization" ? group : null;
  const heading = organization
    ? {
      title: organization.name,
      description: (
        <>
          <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
            {organization.slug && <span className="font-mono">@{organization.slug}</span>}
            <span className="font-mono tabular-nums">{t("projects.count", { count: splitGeneral(organization.projects).repositories.length })}</span>
            {organization.role && <Badge variant="outline">{t(`org.role.${organization.role}` as Key)}</Badge>}
          </span>
          <span className="mt-1 block">{t("projects.org.description")}</span>
        </>
      ),
    }
    : { title: t("nav.projects"), description: t("projects.description") };

  return (
    <ScrollPage>
      <PageHeading eyebrow={t("projects.eyebrow")} title={heading.title} description={heading.description}>
        <LayoutSwitch value={layout} onChange={setLayout} />
        {organization
          ? <OrganizationChatButton organization={{ id: organization.key, name: organization.name }} />
          : !inOrganization && <NewProjectDialog><Button>{t("projects.new")}</Button></NewProjectDialog>}
      </PageHeading>
      {data.projects.length === 0 && group.scope.kind === "personal"
        ? <EmptyText>{t("projects.empty")}</EmptyText>
        : (
          <div className="grid gap-8">
            <div className="-mt-2 flex flex-wrap items-center gap-3">
              <label className="relative ml-auto block w-full min-w-0 sm:w-64">
                <span className="sr-only">{t("projects.search.label")}</span>
                <SearchIcon aria-hidden="true" className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input type="search" value={query} placeholder={t("projects.search.placeholder")} className="h-8 pl-9" onChange={(event) => setQuery(event.target.value)} />
              </label>
            </div>
            {query.trim() && shown.length === 0 && <EmptyText>{t("projects.search.empty")}</EmptyText>}
            {(query.trim() ? shown : [group]).map((item) => <ProjectScopeSection key={item.key} group={item} layout={layout} heading={!organization} />)}
          </div>
        )}
    </ScrollPage>
  );
}
