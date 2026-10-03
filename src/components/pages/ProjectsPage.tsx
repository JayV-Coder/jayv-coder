import { useEffect, useMemo, useState } from "react";
import { SearchIcon } from "lucide-react";
import { filterScopeGroups, groupByScope, loadOrganizations, useOrganizations, type ScopeFilter } from "@/modules/organizations";
import { setLayout, useWorkspace } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { EmptyText } from "@/components/atoms";
import { LayoutSwitch, PageHeading, SegmentedControl } from "@/components/molecules";
import { NewProjectDialog, ProjectScopeSection } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

const SCOPE_KEY = "jayv.projectScope";
const FILTERS: ScopeFilter[] = ["all", "personal", "organizations"];

/** O filtro guardado. Antes ele guardava o id de uma organização: esse valor
 * volta para "todos". */
function storedScope(): ScopeFilter {
  try {
    const stored = localStorage.getItem(SCOPE_KEY);
    return FILTERS.includes(stored as ScopeFilter) ? (stored as ScopeFilter) : "all";
  } catch {
    return "all";
  }
}

/** Os projetos separados por dono: primeiro os pessoais, depois um bloco por
 * organização. O filtro de cima mostra todos, só os pessoais ou só os das
 * organizações; a busca ao lado procura pelo nome, pela pasta e pelos
 * repositórios. */
export function ProjectsPage() {
  const t = useT();
  const { data, layout } = useWorkspace();
  const links = useOrganizations((state) => state.projects);
  const organizations = useOrganizations((state) => state.list);
  const [scope, setScopeState] = useState(storedScope);
  const [query, setQuery] = useState("");
  // O vínculo com a organização depende do que já subiu dos remotes: confere de
  // novo a cada volta à lista.
  useEffect(() => { loadOrganizations().catch((error) => console.error("organizations", error)); }, []);

  const groups = useMemo(() => groupByScope(data.projects, links, organizations), [data.projects, links, organizations]);
  // Sem organização nenhuma, o filtro some e a lista mostra tudo.
  const current = groups.length > 1 ? scope : "all";
  const shown = filterScopeGroups(groups, current, query);
  const personal = groups.find((group) => group.scope.kind === "personal")?.projects.length ?? 0;
  const setScope = (next: ScopeFilter) => {
    setScopeState(next);
    try { localStorage.setItem(SCOPE_KEY, next); } catch { /* só a preferência se perde */ }
  };

  return (
    <ScrollPage>
      <PageHeading eyebrow={t("projects.eyebrow")} title={t("nav.projects")} description={t("projects.description")}>
        <LayoutSwitch value={layout} onChange={setLayout} />
        <NewProjectDialog><Button>{t("projects.new")}</Button></NewProjectDialog>
      </PageHeading>
      {data.projects.length === 0 && organizations.length === 0
        ? <EmptyText>{t("projects.empty")}</EmptyText>
        : (
          <div className="grid gap-8">
            <div className="-mt-2 flex flex-wrap items-center gap-3">
              {groups.length > 1 && (
                <SegmentedControl
                  label={t("projects.scope.label")}
                  value={current}
                  onChange={setScope}
                  options={[
                    { value: "all", label: `${t("projects.scope.all")} · ${data.projects.length}` },
                    { value: "personal", label: `${t("projects.scope.personal")} · ${personal}` },
                    { value: "organizations", label: `${t("projects.scope.organizations")} · ${data.projects.length - personal}` },
                  ]}
                />
              )}
              <label className="relative ml-auto block w-full min-w-0 sm:w-64">
                <span className="sr-only">{t("projects.search.label")}</span>
                <SearchIcon aria-hidden="true" className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input type="search" value={query} placeholder={t("projects.search.placeholder")} className="h-8 pl-9" onChange={(event) => setQuery(event.target.value)} />
              </label>
            </div>
            {shown.length === 0 && <EmptyText>{t("projects.search.empty")}</EmptyText>}
            {shown.map((group) => <ProjectScopeSection key={group.key} group={group} layout={layout} />)}
          </div>
        )}
    </ScrollPage>
  );
}
