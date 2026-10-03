import { BrainIcon } from "lucide-react";
import { chatsOf, createChat, deleteChat, findProject, openChat, useWorkspace } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { EmptyText } from "@/components/atoms";
import { PageHeading } from "@/components/molecules";
import { ChatCard, ChatSearch, ProjectMemoryDialog } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Button } from "@/components/ui/button";

/** A grade de chats é a primeira tela de um projeto aberto. */
export function ChatsPage() {
  const t = useT();
  const { data, activeProjectId } = useWorkspace();
  const project = findProject(data, activeProjectId);
  const chats = chatsOf(data, activeProjectId);
  return (
    <ScrollPage>
      <PageHeading eyebrow={project ? t("header.project", { name: project.name }) : t("chats.eyebrow")} title={t("nav.chats")} description={t("chats.description")}>
        {project && (
          <div className="flex gap-2">
            <ProjectMemoryDialog projectId={project.id}>
              <Button variant="outline"><BrainIcon aria-hidden="true" />{t("memory.open")}</Button>
            </ProjectMemoryDialog>
            <Button onClick={() => void createChat(project.id)}>{t("common.newChat")}</Button>
          </div>
        )}
      </PageHeading>
      {project && chats.length > 0 && <ChatSearch projectId={project.id} />}
      {chats.length === 0
        ? <EmptyText>{t("chats.empty")}</EmptyText>
        : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-4">
            {chats.map((chat) => <ChatCard key={chat.id} chat={chat} onOpen={() => openChat(chat.id)} onDelete={() => void deleteChat(chat.id)} />)}
          </div>
        )}
    </ScrollPage>
  );
}
