import { useEffect, useMemo, useState } from "react";
import { groupByScope, loadOrganizations, useOrganizations } from "@/modules/organizations";
import { setLayout, useWorkspace } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { EmptyText } from "@/components/atoms";
import { LayoutSwitch, PageHeading, SegmentedControl } from "@/components/molecules";
import { NewProjectDialog, ProjectScopeSection } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Button } from "@/components/ui/button";

const SCOPE_KEY = "jayv.projectScope";

function storedScope() {
  try { return localStorage.getItem(SCOPE_KEY) ?? "all"; } catch { return "all"; }
}

/** Os projetos separados por dono: primeiro os pessoais, depois um bloco por
 * organização. O filtro de cima mostra um bloco só. */
export function ProjectsPage() {
  const t = useT();
  const { data, layout } = useWorkspace();
  const links = useOrganizations((state) => state.projects);
  const organizations = useOrganizations((state) => state.list);
  const [scope, setScopeState] = useState(storedScope);
  // O vínculo com a organização depende do que já subiu dos remotes: confere de
  // novo a cada volta à lista.
  useEffect(() => { loadOrganizations().catch((error) => console.error("organizations", error)); }, []);

  const groups = useMemo(() => groupByScope(data.projects, links, organizations), [data.projects, links, organizations]);
  // A organização escolhida pode ter saído da lista: volta para todos.
  const current = groups.some((group) => group.key === scope) ? scope : "all";
  const shown = current === "all" ? groups : groups.filter((group) => group.key === current);
  const setScope = (next: string) => {
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
            {groups.length > 1 && (
              <div className="-mt-2 max-w-full overflow-x-auto">
                <SegmentedControl
                  label={t("projects.scope.label")}
                  value={current}
                  onChange={setScope}
                  options={[
                    { value: "all", label: `${t("projects.scope.all")} · ${data.projects.length}` },
                    ...groups.map((group) => ({
                      value: group.key,
                      label: `${group.scope.kind === "personal" ? t("projects.scope.personal") : group.name} · ${group.projects.length}`,
                    })),
                  ]}
                />
              </div>
            )}
            {shown.map((group) => <ProjectScopeSection key={group.key} group={group} layout={layout} />)}
          </div>
        )}
    </ScrollPage>
  );
}
