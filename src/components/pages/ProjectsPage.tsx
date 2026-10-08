import { useEffect, useMemo, useState } from "react";
import { SearchIcon } from "lucide-react";
import { loadOrganizations, searchScopeGroups, useOrganizations, type ScopeGroup } from "@/modules/organizations";
import { PERSONAL, useEnvironment } from "@/modules/environments";
import type { Project } from "@/modules/core";
import { setLayout, useWorkspace } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { EmptyText } from "@/components/atoms";
import { LayoutSwitch, PageHeading } from "@/components/molecules";
import { NewProjectDialog, ProjectScopeSection } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** Os projetos do ambiente aberto: o banco de cada ambiente só tem os dele, então
 * a lista é um bloco só, com o cabeçalho do dono (a pessoa ou a organização). A
 * busca ao lado procura pelo nome, pela pasta e pelos repositórios. */
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

  return (
    <ScrollPage>
      <PageHeading eyebrow={t("projects.eyebrow")} title={t("nav.projects")} description={t("projects.description")}>
        <LayoutSwitch value={layout} onChange={setLayout} />
        <NewProjectDialog><Button>{t("projects.new")}</Button></NewProjectDialog>
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
            {(query.trim() ? shown : [group]).map((item) => <ProjectScopeSection key={item.key} group={item} layout={layout} />)}
          </div>
        )}
    </ScrollPage>
  );
}
