import { Building2Icon, UserIcon } from "lucide-react";
import type { Project } from "@/modules/core";
import { useProjectRefusals } from "@/modules/connection";
import { useT } from "@/modules/i18n";
import { useOrganizations } from "@/modules/organizations";
import { PathText } from "@/components/atoms";

/** A placa do projeto aberto: de quem ele é, nome, pasta no disco e o que o
 * servidor recusou dele. */
export function ProjectPlate({ project }: { project: Project }) {
  const t = useT();
  const refused = useProjectRefusals(project.id);
  const organization = useOrganizations((state) => state.projects[project.id]);
  return (
    <div className="mb-3 rounded-lg border border-sidebar-border bg-card px-3 py-2.5">
      {organization
        ? (
          <span title={t("org.project.badge", { name: organization.name })} className="mb-1 flex min-w-0 items-center gap-1.5 text-caption font-medium text-success">
            <Building2Icon aria-hidden="true" className="size-3.5 flex-none" />
            <span className="truncate">{organization.name}</span>
          </span>
        )
        : (
          <span className="mb-1 flex items-center gap-1.5 text-caption font-medium text-muted-foreground">
            <UserIcon aria-hidden="true" className="size-3.5 flex-none" />
            {t("project.scope.personal")}
          </span>
        )}
      <strong title={project.name} className="block text-sm leading-tight font-semibold break-words text-foreground">{project.name}</strong>
      {project.rootPath
        ? <PathText title={project.rootPath} className="mt-1 font-mono text-caption leading-snug text-muted-foreground">{project.rootPath}</PathText>
        : <span title={t("project.noFolder.title")} className="mt-1 block text-caption text-muted-foreground">{t("common.noFolder")}</span>}
      {refused > 0 && <span role="status" className="mt-1 block text-caption text-warning">{t("project.refused", { count: refused })}</span>}
    </div>
  );
}
