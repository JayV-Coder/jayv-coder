import { useEnvironmentOrganization } from "@/modules/environments";
import { formatSince, useT } from "@/modules/i18n";
import { useNavigation } from "@/modules/navigation";
import { organizationChatsOf, useOrganizations } from "@/modules/organizations";
import { useFeature } from "@/modules/plans";
import { deleteChat, openChat, useWorkspace } from "@/modules/workspace";
import { ChatRow } from "@/components/molecules";
import { OrganizationChatButton } from "./OrganizationChatButton";

/** Se a coluna dos chats gerais aparece: no ambiente de uma organização, com o
 * chat da organização no plano. */
export function useGeneralChatsColumn() {
  const orgId = useEnvironmentOrganization();
  const allowed = useFeature("orgChat");
  return orgId !== null && allowed;
}

/** Os chats gerais da organização numa coluna própria, entre o menu lateral e
 * a tela: os que trabalham em todos os repositórios dela de uma vez, do mais
 * recente para o mais antigo, com o "+" que começa outro. Fica visível em
 * qualquer tela do ambiente da organização; sem chat geral ainda, o "+" abre o
 * primeiro (pedindo a pasta da organização, se for preciso). */
export function GeneralChatsColumn() {
  const t = useT();
  const orgId = useEnvironmentOrganization();
  const organization = useOrganizations((state) => (orgId ? state.list.find((item) => item.id === orgId) ?? null : null));
  const data = useWorkspace((state) => state.data);
  const activeChatId = useWorkspace((state) => state.activeChatId);
  const view = useNavigation((state) => state.view);
  if (!orgId) return null;
  const chats = organizationChatsOf(data, orgId);
  // O chat só aparece marcado enquanto a conversa dele está na tela.
  const openId = view === "chat" ? activeChatId : null;

  return (
    <section
      aria-labelledby="general-chats-title"
      data-tour="general-chats"
      className="flex h-full min-h-0 flex-col overflow-hidden border-e border-sidebar-border bg-sidebar px-2 pt-4 pb-3 text-sidebar-foreground"
    >
      <div className="flex items-center justify-between ps-2.5 pe-0.5 pb-1.5">
        <h2 id="general-chats-title" className="font-mono text-caption font-medium tracking-wider text-sidebar-muted uppercase">{t("projects.org.general")}</h2>
        {organization && <OrganizationChatButton organization={{ id: organization.id, name: organization.name }} fresh className="size-6 text-sidebar-muted hover:text-sidebar-foreground" />}
      </div>
      <p className="px-2.5 pb-3 text-caption leading-snug text-sidebar-muted">{t("projects.org.generalHint")}</p>
      <div className="grid min-h-0 grid-cols-[minmax(0,1fr)] content-start gap-0.5 overflow-x-hidden overflow-y-auto pe-[3px]">
        {chats.length === 0
          ? <p className="mx-2.5 my-1 text-caption text-sidebar-muted">{t("generalChats.empty")}</p>
          : chats.map((chat) => (
            <ChatRow key={chat.id} chat={chat} open={chat.id === openId} detail={formatSince(chat.updatedAt)} onOpen={() => openChat(chat.id)} onDelete={() => void deleteChat(chat.id)} />
          ))}
      </div>
    </section>
  );
}
