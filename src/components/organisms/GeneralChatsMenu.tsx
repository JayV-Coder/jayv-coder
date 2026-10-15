import { useState } from "react";
import { ChevronDownIcon, MessagesSquareIcon } from "lucide-react";
import { useEnvironmentOrganization } from "@/modules/environments";
import { formatSince, useT } from "@/modules/i18n";
import { useNavigation } from "@/modules/navigation";
import { organizationChatsOf, useOrganizations } from "@/modules/organizations";
import { useFeature } from "@/modules/plans";
import { deleteChat, openChat, useWorkspace } from "@/modules/workspace";
import { ChatRow, NavItem } from "@/components/molecules";
import { cn } from "@/lib/utils";
import { OrganizationChatButton } from "./OrganizationChatButton";

const OPEN_KEY = "jayv.generalChats.open";

function remembered(): boolean {
  try {
    return localStorage.getItem(OPEN_KEY) !== "0";
  } catch {
    return true;
  }
}

/** Se os chats gerais aparecem no menu: no ambiente de uma organização, com o
 * chat da organização no plano. */
export function useGeneralChats() {
  const orgId = useEnvironmentOrganization();
  const allowed = useFeature("orgChat");
  return orgId !== null && allowed;
}

/** Os chats gerais da organização (os que trabalham em todos os repositórios
 * dela de uma vez) como um item "Chats" do menu lateral que abre e fecha, do
 * mais recente para o mais antigo, com o "+" que começa outro. Aberto ou
 * fechado fica lembrado neste aparelho; sem chat geral ainda, o "+" abre o
 * primeiro (pedindo a pasta da organização, se for preciso). */
export function GeneralChatsMenu() {
  const t = useT();
  const orgId = useEnvironmentOrganization();
  const organization = useOrganizations((state) => (orgId ? state.list.find((item) => item.id === orgId) ?? null : null));
  const data = useWorkspace((state) => state.data);
  const activeChatId = useWorkspace((state) => state.activeChatId);
  const view = useNavigation((state) => state.view);
  const [open, setOpen] = useState(remembered);
  if (!orgId) return null;
  const chats = organizationChatsOf(data, orgId);
  // O chat só aparece marcado enquanto a conversa dele está na tela.
  const openId = view === "chat" ? activeChatId : null;
  const current = openId !== null && chats.some((chat) => chat.id === openId);

  const toggle = () => {
    setOpen((value) => {
      try { localStorage.setItem(OPEN_KEY, value ? "0" : "1"); } catch { /* fica só nesta sessão */ }
      return !value;
    });
  };

  return (
    <div data-tour="general-chats" className="grid min-h-0">
      <div className="flex items-center gap-0.5">
        <NavItem active={current && !open} aria-expanded={open} aria-controls="general-chats-list" title={t("projects.org.generalHint")}
          mark={<MessagesSquareIcon />} onClick={toggle} className="min-w-0 flex-1">
          <span className="truncate">{t("nav.chats")}</span>
          <span className="ms-auto flex shrink-0 items-center gap-1.5 text-caption text-faint">
            {chats.length > 0 && <span className="tabular-nums">{chats.length}</span>}
            <ChevronDownIcon aria-hidden="true" className={cn("size-3.5 transition-transform", !open && "-rotate-90")} />
          </span>
        </NavItem>
        {organization && <OrganizationChatButton organization={{ id: organization.id, name: organization.name }} fresh className="size-7 shrink-0 text-sidebar-muted hover:text-sidebar-foreground" />}
      </div>
      {open && (
        <div id="general-chats-list" role="group" aria-label={t("projects.org.general")}
          className="ms-[17px] mb-1 grid max-h-[40vh] min-h-0 grid-cols-[minmax(0,1fr)] content-start gap-0.5 overflow-x-hidden overflow-y-auto border-s border-sidebar-border ps-1.5 pe-[3px] pt-0.5">
          {chats.length === 0
            ? <p className="mx-2.5 my-1 text-caption text-sidebar-muted">{t("generalChats.empty")}</p>
            : chats.map((chat) => (
              <ChatRow key={chat.id} chat={chat} open={chat.id === openId} detail={formatSince(chat.updatedAt)} onOpen={() => openChat(chat.id)} onDelete={() => void deleteChat(chat.id)} />
            ))}
        </div>
      )}
    </div>
  );
}
