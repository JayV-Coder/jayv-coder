import { ActivityIcon, ArrowLeftIcon, Building2Icon, ChartColumnIcon, FolderKanbanIcon, MessagesSquareIcon, PlusIcon, SettingsIcon } from "lucide-react";
import { useNavigation, navigate } from "@/modules/navigation";
import { chatsOf, createChat, deleteChat, findProject, leaveProject, openChat, recentChats, useWorkspace } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { BrandMark, LogoIcon, UserAvatar } from "@/components/atoms";
import { ChatRow, NavItem, ProjectPlate } from "@/components/molecules";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useAuth } from "@/modules/auth";
import { useProfile } from "@/modules/profile";
import { useOrganizations } from "@/modules/organizations";
import { openStats } from "@/modules/usage";
import { cn } from "@/lib/utils";
import { displayName } from "./ProfileCard";
import { NotificationBell } from "./NotificationBell";

/** A lateral. Sem projeto aberto, ela é o menu principal; com projeto, só mostra
 * a placa dele, a portaria e os chats recentes. */
export function Sidebar() {
  const t = useT();
  const email = useAuth((state) => state.email);
  const profile = useAuth((state) => state.profile);
  const account = useProfile((state) => state.profile);
  const name = displayName(account, profile, email);
  const view = useNavigation((state) => state.view);
  const invites = useOrganizations((state) => state.incoming.length);
  const { data, activeProjectId, activeChatId } = useWorkspace();
  const project = findProject(data, activeProjectId);
  // O chat só aparece selecionado enquanto a conversa dele está na tela: na
  // grade de chats nenhuma linha fica marcada.
  const openId = view === "chat" ? activeChatId : null;
  const chats = project ? recentChats(chatsOf(data, project.id), openId) : [];

  return (
    <aside className="flex h-full flex-col overflow-hidden border-e border-sidebar-border bg-sidebar px-3 pt-4 pb-3 text-sidebar-foreground">
      <div className="flex items-center gap-2.5 px-1.5 pb-5">
        <BrandMark />
        <div className="flex flex-col">
          <strong className="text-sm font-semibold">JayV</strong>
          <small className="text-caption text-sidebar-muted">{t("brand.tagline")}</small>
        </div>
      </div>

      {!project ? (
        <nav className="grid gap-0.5">
          <NavItem active={view === "projects"} mark={<FolderKanbanIcon />} shortcut="projects" onClick={() => navigate("projects")}>{t("nav.projects")}</NavItem>
          <NavItem active={view === "organizations" || view === "organization"} mark={<Building2Icon />} shortcut={invites > 0 ? undefined : "organizations"} onClick={() => navigate("organizations")}>
            <span className="flex-1">{t("nav.organizations")}</span>
            {invites > 0 && (
              <span title={t("org.invites.count", { count: invites })} className="rounded-md bg-accent px-1.5 font-mono text-caption font-semibold text-accent-foreground tabular-nums">{invites}</span>
            )}
          </NavItem>
          <NavItem active={view === "stats"} mark={<ChartColumnIcon />} shortcut="stats" onClick={() => openStats({ kind: "global" })}>{t("nav.stats")}</NavItem>
          <NavItem active={view === "status"} mark={<ActivityIcon />} shortcut="system" onClick={() => navigate("status")}>{t("nav.system")}</NavItem>
        </nav>
      ) : (
        <div className="flex min-h-0 flex-col">
          <button
            type="button"
            onClick={leaveProject}
            className="mb-2 flex h-8 w-full items-center gap-2 rounded-md px-2.5 text-start text-xs font-medium text-sidebar-muted transition-colors outline-none hover:bg-sidebar-accent/70 hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-ring"
          >
            <ArrowLeftIcon aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            {t("nav.allProjects")}
          </button>
          <ProjectPlate project={project} />
          <NavItem active={view === "chats"} mark={<MessagesSquareIcon />} onClick={() => navigate("chats")}>{t("nav.chats")}</NavItem>
          <NavItem
            active={view === "gate"}
            className="mb-1"
            shortcut="gate"
            mark={<LogoIcon className="size-[18px] [&_.logo-halo]:stroke-ask [&_.logo-lamp]:fill-ask" />}
            onClick={() => navigate("gate")}
          >
            {t("nav.gate")}
          </NavItem>
          <NavItem active={view === "stats"} mark={<ChartColumnIcon />} onClick={() => openStats({ kind: "project", id: project.id })}>{t("nav.stats")}</NavItem>
          <div className="flex items-center justify-between ps-2.5 pe-1 pt-5 pb-1.5 font-mono text-caption font-medium tracking-wider text-sidebar-muted uppercase">
            {t("nav.recentChats")}
            <button type="button" title={t("common.newChat")} aria-label={t("common.newChat")} onClick={() => void createChat(project.id)} className="grid size-6 place-items-center rounded-md text-sidebar-muted transition-colors outline-none hover:bg-sidebar-accent hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-ring"><PlusIcon aria-hidden="true" className="size-4" /></button>
          </div>
          <div className="grid min-h-0 grid-cols-[minmax(0,1fr)] gap-0.5 overflow-x-hidden overflow-y-auto pe-[3px]">
            {chats.length === 0
              ? <p className="mx-3 my-1 text-caption text-sidebar-muted">{t("nav.noChats")}</p>
              : chats.map((chat) => (
                <ChatRow key={chat.id} chat={chat} open={chat.id === openId} onOpen={() => openChat(chat.id)} onDelete={() => void deleteChat(chat.id)} />
              ))}
          </div>
        </div>
      )}

      {/* A conta abre o perfil; o sino, as notificações; a engrenagem, as
          configurações — o idioma mora lá dentro. */}
      <div className="mt-auto grid gap-2 border-t border-sidebar-border pt-3">
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            aria-current={view === "profile" ? "page" : undefined}
            title={t("nav.profile")}
            onClick={() => navigate("profile")}
            className={cn(
              "flex min-w-0 flex-1 items-center gap-2.5 rounded-md px-2 py-1.5 text-start transition-colors outline-none hover:bg-sidebar-accent/70 focus-visible:ring-2 focus-visible:ring-ring",
              view === "profile" && "bg-sidebar-accent",
            )}
          >
            <UserAvatar name={name} src={profile?.avatarUrl} className="size-7 text-xs" />
            <span className="grid min-w-0">
              <span className="truncate text-[13px] font-medium text-sidebar-foreground">{name}</span>
              {email && name !== email && <span className="truncate text-caption text-sidebar-muted">{email}</span>}
            </span>
          </button>
          <NotificationBell />
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                type="button"
                aria-label={t("nav.settings")}
                aria-current={view === "settings" ? "page" : undefined}
                onClick={() => navigate("settings")}
                className={cn(
                  "grid size-9 shrink-0 place-items-center rounded-md text-sidebar-muted transition-colors outline-none hover:bg-sidebar-accent/70 hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-ring",
                  view === "settings" && "bg-sidebar-accent text-sidebar-foreground",
                )}
              >
                <SettingsIcon aria-hidden="true" className="size-[18px]" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="top">{t("nav.settings")}</TooltipContent>
          </Tooltip>
        </div>
      </div>
    </aside>
  );
}
