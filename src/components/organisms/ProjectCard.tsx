import { Trash2Icon } from "lucide-react";
import type { Chat, Project } from "@/modules/core";
import { useProjectRefusals } from "@/modules/connection";
import { useOrganizations } from "@/modules/organizations";
import { formatSince, useT } from "@/modules/i18n";
import { FolderIcon, PathText } from "@/components/atoms";
import { ConfirmAction } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { ProjectUsageLine } from "./ProjectUsageLine";
import { YardCard } from "./YardCard";

export function ProjectCard({ project, chats, onOpen, onNewChat, onDelete }: {
  project: Project;
  chats: Chat[];
  onOpen: () => void;
  onNewChat: () => void;
  onDelete: () => void;
}) {
  const t = useT();
  const last = chats[0]?.updatedAt ?? project.createdAt;
  const count = chats.length;
  const refused = useProjectRefusals(project.id);
  const organization = useOrganizations((state) => state.projects[project.id]);
  const policed = useOrganizations((state) => state.policed[project.id] === true);
  return (
    <YardCard
      onOpen={onOpen}
      actions={(
        <>
          <Button variant="ghost" size="sm" onClick={onNewChat}>{t("common.newChat")}</Button>
          <ConfirmAction
            title={t("project.delete.title")}
            description={t("project.delete.description", { name: project.name, count })}
            onConfirm={onDelete}
          >
            <Button variant="ghost" size="sm" className="ms-auto text-muted-foreground hover:text-destructive">
            <Trash2Icon aria-hidden="true" />
            {t("common.delete")}
          </Button>
          </ConfirmAction>
        </>
      )}
    >
      <span className="flex flex-wrap items-baseline gap-2">
        <span className="text-lg font-semibold tracking-tight break-words">{project.name}</span>
        {organization && (
          <span title={t("org.project.badge", { name: organization.name })} className="rounded-md border border-border px-1.5 font-mono text-caption text-success">@{organization.slug}</span>
        )}
        {organization && policed && (
          <span title={t("policy.badge.hint", { slug: organization.slug })} className="rounded-md border border-border px-1.5 text-caption text-muted-foreground">{t("policy.badge")}</span>
        )}
      </span>
      {project.rootPath ? (
        <span title={project.rootPath} className="flex min-w-0 items-center gap-2 text-muted-foreground">
          <FolderIcon className="size-4 flex-none" />
          <PathText className="text-xs">{project.rootPath}</PathText>
        </span>
      ) : <span className="text-xs text-muted-foreground">{t("common.noFolder")}</span>}
      <span className="flex items-baseline gap-3 text-xs text-muted-foreground">
        <b className="font-normal text-foreground">{t("project.chats", { count })}</b>
        <i className="not-italic">{t("project.lastActivity", { date: formatSince(last) })}</i>
      </span>
      <ProjectUsageLine projectId={project.id} />
      {refused > 0 && <span role="status" className="text-xs text-warning">{t("project.refused", { count: refused })}</span>}
    </YardCard>
  );
}
