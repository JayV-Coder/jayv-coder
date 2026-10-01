import type { Project } from "@/modules/core";
import { useProjectRefusals } from "@/modules/connection";
import { useT } from "@/modules/i18n";
import { PathText } from "@/components/atoms";

/** A placa do projeto aberto: nome, pasta no disco e o que o servidor recusou dele. */
export function ProjectPlate({ project }: { project: Project }) {
  const t = useT();
  const refused = useProjectRefusals(project.id);
  return (
    <div className="mb-3.5 rounded-e-[10px] border border-s-[3px] border-[#2b3a2e] border-s-[#7bd985] bg-[linear-gradient(100deg,#16211a,#111614)] px-3 py-3">
      <strong title={project.name} className="block text-base leading-tight tracking-[-0.01em] break-words text-[#f1f7f2]">{project.name}</strong>
      {project.rootPath
        ? <PathText title={project.rootPath} className="mt-1.5 text-[11px] leading-snug text-[#8ba892]">{project.rootPath}</PathText>
        : <span title={t("project.noFolder.title")} className="mt-1.5 block text-[11px] text-[#5c665e]">{t("common.noFolder")}</span>}
      {refused > 0 && <span role="status" className="mt-1.5 block text-[11px] text-[#c9a86a]">{t("project.refused", { count: refused })}</span>}
    </div>
  );
}
