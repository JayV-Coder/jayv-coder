import { findChat, findProject, useWorkspace } from "@/modules/workspace";
import { ChatUsageBar, Composer, Timeline } from "@/components/organisms";

/** A conversa numa coluna só, de leitura confortável: as mensagens rolam em
 * cima e a caixa de escrita fica presa embaixo, com o gasto do chat logo
 * abaixo dela. */
export function ChatPage() {
  const { data, activeProjectId, activeChatId } = useWorkspace();
  const chat = findChat(data, activeChatId);
  const project = findProject(data, activeProjectId);
  return (
    <>
      <Timeline chat={chat} project={project} />
      <div className="mx-auto w-full max-w-4xl flex-none px-5 pb-3">
        <Composer chat={chat} />
        {chat && <ChatUsageBar chatId={chat.id} />}
      </div>
    </>
  );
}
