import { Building2Icon, FolderDownIcon, UserIcon } from "lucide-react";
import type { Project } from "@/modules/core";
import { formatSince, useT, type Key } from "@/modules/i18n";
import { openOrganization, organizationChatsOf, splitGeneral, type ScopeGroup } from "@/modules/organizations";
import { reportError } from "@/modules/feedback";
import { PERSONAL, useEnvironment } from "@/modules/environments";
import { chatTitle, chatsOf, createChat, deleteProject, openChat, openProject, useWorkspace, type Layout } from "@/modules/workspace";
import { EmptyText } from "@/components/atoms";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { ProjectCard } from "./ProjectCard";

/** Um bloco da lista de projetos: os pessoais ou os de uma organização, com o
 * cabeçalho dizendo de quem são. No ambiente de uma organização o cabeçalho
 * fica no alto da página (`heading` desligado), com o chat dela. */
export function ProjectScopeSection({ group, layout, heading = true }: { group: ScopeGroup<Project>; layout: Layout; heading?: boolean }) {
  const t = useT();
  const data = useWorkspace((state) => state.data);
  const personal = group.scope.kind === "personal";
  // Excluir só no ambiente pessoal: o projeto de uma organização só se exclui
  // no site, por owner ou maintainer — nem enquanto a lista delas chega.
  const deletable = useEnvironment((state) => state.active === PERSONAL) && personal;
  const Icon = personal ? UserIcon : Building2Icon;
  // Os chats gerais (todos os repositórios juntos) ficam num bloco à parte dos
  // projetos de cada repositório.
  const { repositories } = splitGeneral(group.projects);
  const generalChats = personal ? [] : organizationChatsOf(data, group.key);
  const count = repositories.length;
  return (
    <section aria-labelledby={heading ? `scope-${group.key}` : undefined} className="grid gap-3">
      {heading && <header className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-border pb-2">
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
      </header>}
      {generalChats.length > 0 && (
        <div className="grid gap-2">
          <h4 className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">{t("projects.org.general")}</h4>
          <p className="text-xs text-muted-foreground">{t("projects.org.generalHint")}</p>
          <ul className="grid gap-1.5">
            {generalChats.slice(0, 5).map((chat) => (
              <li key={chat.id}>
                <button
                  type="button"
                  onClick={() => openChat(chat.id)}
                  className="flex w-full min-w-0 items-baseline justify-between gap-3 rounded-md border border-border/60 px-3 py-2 text-start hover:bg-secondary focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                >
                  <span className="truncate text-sm">{chatTitle(chat)}</span>
                  <span className="shrink-0 text-caption text-muted-foreground">{formatSince(chat.updatedAt)}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
      {generalChats.length > 0 && count > 0 && <h4 className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">{t("projects.org.repositories")}</h4>}
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
            {repositories.map((project) => (
              <ProjectCard
                key={project.id}
                project={project}
                chats={chatsOf(data, project.id)}
                onOpen={() => openProject(project.id)}
                onNewChat={() => void createChat(project.id)}
                onDelete={deletable ? () => void deleteProject(project.id) : undefined}
              />
            ))}
          </div>
        )}
    </section>
  );
}
