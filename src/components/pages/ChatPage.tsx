import { useEffect } from "react";
import { loadLive, useLive } from "@/modules/live";
import { useFeature } from "@/modules/plans";
import { findChat, findProject, useWorkspace } from "@/modules/workspace";
import { ChatUsageBar, Composer, LivePanel, ProgressPanel, Timeline } from "@/components/organisms";

/** A conversa numa coluna só, de leitura confortável: as mensagens rolam em
 * cima e a caixa de escrita fica presa embaixo, com o gasto do chat logo
 * abaixo dela. À esquerda, junto ao menu lateral, o painel de andamento do
 * JayV. Com o painel de código aberto, à direita da conversa ficam os
 * arquivos que o agente mexe e os repositórios da pasta do chat. */
export function ChatPage() {
  const { data, activeProjectId, activeChatId } = useWorkspace();
  const chat = findChat(data, activeChatId);
  const project = findProject(data, activeProjectId);
  const liveAllowed = useFeature("liveFiles");
  const live = useLive((state) => (activeChatId ? state.panel[activeChatId] === true : false));

  useEffect(() => {
    if (activeChatId) void loadLive(activeChatId).catch((error) => console.error("live files", error));
  }, [activeChatId]);

  return (
    <div className="flex min-h-0 flex-1">
      {chat && (
        <ProgressPanel chat={chat} />
      )}
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <Timeline chat={chat} project={project} />
        <div className="mx-auto w-full max-w-4xl flex-none px-5 pb-3">
          <Composer chat={chat} />
          {chat && <ChatUsageBar chatId={chat.id} />}
        </div>
      </div>
      {chat && live && project?.rootPath.trim() && <LivePanel chat={chat} project={project} files={liveAllowed} />}
    </div>
  );
}
