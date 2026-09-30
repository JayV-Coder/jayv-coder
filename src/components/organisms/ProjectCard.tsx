import type { Chat, Project } from "@/modules/core";
import { formatSince, useT } from "@/modules/i18n";
import { FolderIcon, PathText } from "@/components/atoms";
import { ConfirmAction } from "@/components/molecules";
import { Button } from "@/components/ui/button";
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
            <Button variant="ghost" size="sm" className="text-muted-foreground hover:text-destructive">{t("common.delete")}</Button>
          </ConfirmAction>
        </>
      )}
    >
      <span className="text-lg font-bold tracking-[-0.01em] break-words">{project.name}</span>
      {project.rootPath ? (
        <span title={project.rootPath} className="flex min-w-0 items-center gap-2 text-[#8ba892]">
          <FolderIcon className="size-4 flex-none" />
          <PathText className="text-[11.5px]">{project.rootPath}</PathText>
        </span>
      ) : <span className="text-xs text-[#5c665e]">{t("common.noFolder")}</span>}
      <span className="flex items-baseline gap-3 text-xs text-muted-foreground">
        <b className="font-normal text-foreground">{t("project.chats", { count })}</b>
        <i className="not-italic">{t("project.lastActivity", { date: formatSince(last) })}</i>
      </span>
    </YardCard>
  );
}
