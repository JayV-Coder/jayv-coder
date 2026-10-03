import { Building2Icon, FolderDownIcon, UserIcon } from "lucide-react";
import type { Project } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { openOrganization, type ScopeGroup } from "@/modules/organizations";
import { reportError } from "@/modules/feedback";
import { chatsOf, createChat, deleteProject, openProject, useWorkspace, type Layout } from "@/modules/workspace";
import { EmptyText } from "@/components/atoms";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { ProjectCard } from "./ProjectCard";

/** Um bloco da lista de projetos: os pessoais ou os de uma organização, com o
 * cabeçalho dizendo de quem são e o caminho para a organização. */
export function ProjectScopeSection({ group, layout }: { group: ScopeGroup<Project>; layout: Layout }) {
  const t = useT();
  const data = useWorkspace((state) => state.data);
  const personal = group.scope.kind === "personal";
  const Icon = personal ? UserIcon : Building2Icon;
  const count = group.projects.length;
  return (
    <section aria-labelledby={`scope-${group.key}`} className="grid gap-3">
      <header className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-border pb-2">
        <span aria-hidden="true" className={cn("grid size-8 place-items-center rounded-md border border-border", personal ? "bg-secondary text-foreground" : "bg-card text-success")}>
          <Icon className="size-4" />
        </span>
        <div className="min-w-0 flex-1">
          <h3 id={`scope-${group.key}`} className="flex flex-wrap items-baseline gap-2 text-h4 font-semibold">
            <span className="break-words">{personal ? t("projects.personal.title") : group.name}</span>
            {group.slug && <span className="font-mono text-caption font-normal text-muted-foreground">@{group.slug}</span>}
            <span className="font-mono text-caption font-normal text-muted-foreground tabular-nums">{t("projects.count", { count })}</span>
          </h3>
          <p className="text-xs text-muted-foreground">{personal ? t("projects.personal.description") : t("projects.org.description")}</p>
        </div>
        {group.role && <Badge variant="outline">{t(`org.role.${group.role}` as Key)}</Badge>}
        {!personal && group.role && (
          <Button variant="ghost" size="sm" onClick={() => void openOrganization(group.key).catch(reportError)}>{t("projects.org.open")}</Button>
        )}
      </header>
      {count === 0
        ? (
          <div className="flex flex-wrap items-center gap-3">
            <EmptyText className="min-w-0 flex-1 text-xs">{personal ? t("projects.personal.empty") : t("projects.org.empty")}</EmptyText>
            {!personal && group.role && (
              <Button variant="outline" size="sm" onClick={() => void openOrganization(group.key, "repositories").catch(reportError)}>
                <FolderDownIcon />{t("projects.org.bring")}
              </Button>
            )}
          </div>
        )
        : (
          <div className={cn("grid gap-4", layout === "grid" ? "grid-cols-[repeat(auto-fill,minmax(280px,1fr))]" : "grid-cols-1")}>
            {group.projects.map((project) => (
              <ProjectCard
                key={project.id}
                project={project}
                chats={chatsOf(data, project.id)}
                onOpen={() => openProject(project.id)}
                onNewChat={() => void createChat(project.id)}
                onDelete={() => void deleteProject(project.id)}
              />
            ))}
          </div>
        )}
    </section>
  );
}
