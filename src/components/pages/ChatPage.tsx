import { useEffect } from "react";
import { loadLive, useLive } from "@/modules/live";
import { findChat, findProject, useWorkspace } from "@/modules/workspace";
import { ChatUsageBar, Composer, LivePanel, RepositoryPanel, Timeline } from "@/components/organisms";

/** A conversa numa coluna só, de leitura confortável: as mensagens rolam em
 * cima e a caixa de escrita fica presa embaixo, com o gasto do chat logo
 * abaixo dela. No chat da organização, os repositórios que ele alcança ficam
 * presos no topo. Com o painel "Ao vivo" aberto, os arquivos que o agente
 * mexe ficam à direita da conversa. */
export function ChatPage() {
  const { data, activeProjectId, activeChatId } = useWorkspace();
  const chat = findChat(data, activeChatId);
  const project = findProject(data, activeProjectId);
  const live = useLive((state) => (activeChatId ? state.panel[activeChatId] === true : false));

  useEffect(() => {
    if (activeChatId) void loadLive(activeChatId).catch((error) => console.error("live files", error));
  }, [activeChatId]);

  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        {project?.orgId && (
          <div className="mx-auto w-full max-w-4xl flex-none px-5">
            <RepositoryPanel project={project} chat={chat} />
          </div>
        )}
        <Timeline chat={chat} project={project} />
        <div className="mx-auto w-full max-w-4xl flex-none px-5 pb-3">
          <Composer chat={chat} />
          {chat && <ChatUsageBar chatId={chat.id} />}
        </div>
      </div>
      {chat && live && project?.rootPath.trim() && <LivePanel chat={chat} />}
    </div>
  );
}
