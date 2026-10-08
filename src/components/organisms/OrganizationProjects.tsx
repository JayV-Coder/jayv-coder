import { ChevronRightIcon } from "lucide-react";
import { useT } from "@/modules/i18n";
import { openProject, useWorkspace } from "@/modules/workspace";
import { FolderIcon, PathText } from "@/components/atoms";
import { SettingsSection } from "@/components/molecules";

/** Os projetos deste computador que entraram na organização: a volta da
 * organização para o trabalho. */
export function OrganizationProjects() {
  const t = useT();
  // O banco do ambiente da organização só tem projetos dela.
  const projects = useWorkspace((state) => state.data.projects);
  return (
    <SettingsSection title={t("org.projects.title")} description={t("org.projects.description")}>
      {projects.length === 0
        ? <p className="text-sm text-muted-foreground">{t("projects.org.empty")}</p>
        : (
          <ul className="grid gap-2">
            {projects.map((project) => (
              <li key={project.id}>
                <button type="button" onClick={() => openProject(project.id)}
                  className="flex w-full items-center gap-3 rounded-md border border-border/60 px-3 py-2.5 text-start transition-colors outline-none hover:bg-secondary focus-visible:ring-2 focus-visible:ring-ring">
                  <FolderIcon className="size-5 flex-none text-muted-foreground" />
                  <span className="grid min-w-0 flex-1">
                    <span className="truncate text-sm font-medium">{project.name}</span>
                    {project.rootPath
                      ? <PathText title={project.rootPath} className="text-xs text-muted-foreground">{project.rootPath}</PathText>
                      : <span className="text-xs text-muted-foreground">{t("common.noFolder")}</span>}
                  </span>
                  <ChevronRightIcon aria-hidden="true" className="size-4 flex-none text-muted-foreground rtl:-scale-x-100" />
                </button>
              </li>
            ))}
          </ul>
        )}
    </SettingsSection>
  );
}
