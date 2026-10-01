import { findChat, findProject, useWorkspace } from "@/modules/workspace";
import { ChatUsageBar, Composer, Timeline } from "@/components/organisms";

export function ChatPage() {
  const { data, activeProjectId, activeChatId } = useWorkspace();
  const chat = findChat(data, activeChatId);
  const project = findProject(data, activeProjectId);
  return (
    <>
      <Timeline chat={chat} project={project} />
      {chat && <ChatUsageBar chatId={chat.id} />}
      <Composer chat={chat} />
    </>
  );
}
