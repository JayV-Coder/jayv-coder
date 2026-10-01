import { useNavigation, navigate } from "@/modules/navigation";
import { chatsOf, createChat, deleteChat, findProject, leaveProject, openChat, recentChats, useWorkspace } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { BrandMark, LogoIcon } from "@/components/atoms";
import { ChatRow, ConnectionNote, LanguageSelect, NavItem, ProjectPlate } from "@/components/molecules";
import { signOut, useAuth } from "@/modules/auth";
import { openStats } from "@/modules/usage";

/** A lateral. Sem projeto aberto, ela é o menu principal; com projeto, só mostra
 * a placa dele, a portaria e os chats recentes. */
export function Sidebar() {
  const t = useT();
  const email = useAuth((state) => state.email);
  const view = useNavigation((state) => state.view);
  const { data, activeProjectId, activeChatId } = useWorkspace();
  const project = findProject(data, activeProjectId);
  // O chat só aparece selecionado enquanto a conversa dele está na tela: na
  // grade de chats nenhuma linha fica marcada.
  const openId = view === "chat" ? activeChatId : null;
  const chats = project ? recentChats(chatsOf(data, project.id), openId) : [];

  return (
    <aside className="flex h-screen flex-col overflow-hidden border-e border-[#252a26] bg-[rgba(8,10,9,.9)] px-[15px] pt-[25px] pb-[18px]">
      <div className="flex items-center gap-3 px-2.5 pb-[25px]">
        <BrandMark />
        <div className="flex flex-col">
          <strong>JayV</strong>
          <small className="text-xs text-muted-foreground">{t("brand.tagline")}</small>
        </div>
      </div>

      {!project ? (
        <nav className="grid gap-[5px] border-b border-[#202521] pb-5">
          <NavItem active={view === "projects"} mark="▦" onClick={() => navigate("projects")}>{t("nav.projects")}</NavItem>
          <NavItem active={view === "stats"} mark="▥" onClick={() => openStats({ kind: "global" })}>{t("nav.stats")}</NavItem>
          <NavItem active={view === "status"} mark="◉" onClick={() => navigate("status")}>{t("nav.system")}</NavItem>
        </nav>
      ) : (
        <div className="flex min-h-0 flex-col">
          <button
            type="button"
            onClick={leaveProject}
            className="mb-2.5 flex w-full items-center gap-[9px] rounded-[9px] border border-[#2b322d] px-[11px] py-[9px] text-start text-xs text-[#aab4ac] hover:border-[#4e6353] hover:bg-[#161c17] hover:text-[#eef5ef]"
          >
            <span className="text-sm leading-none text-[#7bd985] rtl:-scale-x-100">←</span>
            {t("nav.allProjects")}
          </button>
          <ProjectPlate project={project} />
          <NavItem active={view === "chats"} mark="▤" onClick={() => navigate("chats")}>{t("nav.chats")}</NavItem>
          <NavItem
            active={view === "gate"}
            className="mb-1"
            mark={<LogoIcon className="size-[17px] [&_.logo-halo]:stroke-ask [&_.logo-lamp]:fill-ask" />}
            onClick={() => navigate("gate")}
          >
            {t("nav.gate")}
          </NavItem>
          <NavItem active={view === "stats"} mark="▥" onClick={() => openStats({ kind: "project", id: project.id })}>{t("nav.stats")}</NavItem>
          <div className="flex items-center justify-between px-[9px] pt-5 pb-[9px] text-[10px] font-bold tracking-[0.16em] text-[#657068] uppercase">
            {t("nav.recentChats")}
            <button type="button" title={t("common.newChat")} onClick={() => void createChat(project.id)} className="text-[19px] leading-none text-[#849087] hover:text-[#a4f4a9]">+</button>
          </div>
          <div className="grid min-h-0 grid-cols-[minmax(0,1fr)] gap-0.5 overflow-x-hidden overflow-y-auto pe-[3px]">
            {chats.length === 0
              ? <p className="mx-[15px] my-[5px] text-[11px] text-[#555e57]">{t("nav.noChats")}</p>
              : chats.map((chat) => (
                <ChatRow key={chat.id} chat={chat} open={chat.id === openId} onOpen={() => openChat(chat.id)} onDelete={() => void deleteChat(chat.id)} />
              ))}
          </div>
        </div>
      )}

      <div className="mt-auto grid gap-2 border-t border-[#222723] pt-3">
        <NavItem active={view === "settings"} mark="⚙" onClick={() => navigate("settings")}>{t("nav.settings")}</NavItem>
        <LanguageSelect />
        <ConnectionNote />
        <button type="button" onClick={() => void signOut()} title={email ?? undefined} className="truncate px-1 text-start text-[11px] text-[#555e57] hover:text-[#c9d1cb]">
          {t("auth.signOut")}{email ? ` · ${email}` : ""}
        </button>
      </div>
    </aside>
  );
}
