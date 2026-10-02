import type { Project } from "@/modules/core";
import { useProjectRefusals } from "@/modules/connection";
import { useT } from "@/modules/i18n";
import { PathText } from "@/components/atoms";

/** A placa do projeto aberto: nome, pasta no disco e o que o servidor recusou dele. */
export function ProjectPlate({ project }: { project: Project }) {
  const t = useT();
  const refused = useProjectRefusals(project.id);
  return (
    <div className="mb-3 rounded-lg border border-sidebar-border bg-card px-3 py-2.5">
      <strong title={project.name} className="block text-sm leading-tight font-semibold tracking-tight break-words text-foreground">{project.name}</strong>
      {project.rootPath
        ? <PathText title={project.rootPath} className="mt-1 font-mono text-caption leading-snug text-muted-foreground">{project.rootPath}</PathText>
        : <span title={t("project.noFolder.title")} className="mt-1 block text-caption text-muted-foreground">{t("common.noFolder")}</span>}
      {refused > 0 && <span role="status" className="mt-1 block text-caption text-warning">{t("project.refused", { count: refused })}</span>}
    </div>
  );
}
