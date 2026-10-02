import { SettingsIcon } from "lucide-react";
import { useNavigation, navigate } from "@/modules/navigation";
import { chatsOf, createChat, deleteChat, findProject, leaveProject, openChat, recentChats, useWorkspace } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { BrandMark, LogoIcon, UserAvatar } from "@/components/atoms";
import { ChatRow, ConnectionNote, NavItem, ProjectPlate } from "@/components/molecules";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useAuth } from "@/modules/auth";
import { openStats } from "@/modules/usage";
import { cn } from "@/lib/utils";
import { displayName } from "./ProfileCard";

/** A lateral. Sem projeto aberto, ela é o menu principal; com projeto, só mostra
 * a placa dele, a portaria e os chats recentes. */
export function Sidebar() {
  const t = useT();
  const email = useAuth((state) => state.email);
  const profile = useAuth((state) => state.profile);
  const name = displayName(profile, email);
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

      {/* A conta abre o perfil; a engrenagem, as configurações — o idioma
          mora lá dentro. */}
      <div className="mt-auto grid gap-2.5 border-t border-[#222723] pt-3">
        <ConnectionNote />
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            aria-current={view === "profile" ? "page" : undefined}
            title={t("nav.profile")}
            onClick={() => navigate("profile")}
            className={cn(
              "flex min-w-0 flex-1 items-center gap-2.5 rounded-[9px] px-2 py-1.5 text-start transition-colors hover:bg-accent",
              view === "profile" && "bg-accent",
            )}
          >
            <UserAvatar name={name} src={profile?.avatarUrl} className="size-8 text-sm" />
            <span className="grid min-w-0">
              <span className="truncate text-[13px] font-medium text-[#dfe6e0]">{name}</span>
              {email && name !== email && <span className="truncate text-[11px] text-[#657068]">{email}</span>}
            </span>
          </button>
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                type="button"
                aria-label={t("nav.settings")}
                aria-current={view === "settings" ? "page" : undefined}
                onClick={() => navigate("settings")}
                className={cn(
                  "grid size-9 shrink-0 place-items-center rounded-[9px] text-[#8f9991] transition-colors hover:bg-accent hover:text-accent-foreground",
                  view === "settings" && "bg-accent text-[#a4f4a9]",
                )}
              >
                <SettingsIcon className="size-[18px]" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="top">{t("nav.settings")}</TooltipContent>
          </Tooltip>
        </div>
      </div>
    </aside>
  );
}
