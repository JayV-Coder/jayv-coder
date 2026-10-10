import {
  ActivityIcon, ChartColumnIcon, CircleHelpIcon, CreditCardIcon, DoorOpenIcon, FolderGit2Icon, FolderKanbanIcon, MessagesSquareIcon,
  PlugIcon, PlusIcon, SettingsIcon, SparklesIcon, UsersIcon, type LucideIcon,
} from "lucide-react";
import { useNavigation, navigate } from "@/modules/navigation";
import { chatsOf, createChat, deleteChat, findProject, leaveProject, openChat, recentChats, useWorkspace } from "@/modules/workspace";
import { useT } from "@/modules/i18n";
import { startTourHere, tourForView, TOURS, startTour } from "@/modules/tutorial";
import { BackMark, BrandMark, LogoIcon, UserAvatar } from "@/components/atoms";
import { ChatRow, NavItem, OrganizationPlate, ProjectPlate } from "@/components/molecules";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useAuth } from "@/modules/auth";
import { useProfile } from "@/modules/profile";
import { isGeneralProject, openOrganization, ORGANIZATION_TABS, useOrganizations, type OrganizationTab } from "@/modules/organizations";
import { useEnvironmentOrganization } from "@/modules/environments";
import { reportError } from "@/modules/feedback";
import { openStats } from "@/modules/usage";
import { allows, useEntitlements } from "@/modules/plans";
import { cn } from "@/lib/utils";
import { displayName } from "./ProfileCard";
import { EnvironmentSwitch } from "./EnvironmentSwitch";
import { useGeneralChatsColumn } from "./GeneralChatsColumn";
import { NotificationBell } from "./NotificationBell";

/** O ícone de cada aba da organização no menu, os mesmos da página dela. */
const TAB_ICONS: Record<OrganizationTab, LucideIcon> = {
  stats: ChartColumnIcon,
  gate: DoorOpenIcon,
  members: UsersIcon,
  repositories: FolderGit2Icon,
  mcp: PlugIcon,
  skills: SparklesIcon,
};

/** A lateral. Sem projeto aberto, ela é o menu principal; com projeto, só mostra
 * a placa dele, a portaria e os chats recentes. No ambiente de uma
 * organização, o menu principal é o dela: a placa da organização no alto e as
 * abas dela como itens; Sistema e Planos ficam só no pessoal. As
 * organizações se abrem pelo seletor de ambiente, e os convites chegam no
 * sino. */
export function Sidebar() {
  const t = useT();
  const email = useAuth((state) => state.email);
  const profile = useAuth((state) => state.profile);
  const account = useProfile((state) => state.profile);
  const photo = useProfile((state) => state.photo);
  const name = displayName(account, profile, email);
  const view = useNavigation((state) => state.view);
  // O que o plano (ou o admin) desligou some do menu; Planos fica sempre.
  const entitlements = useEntitlements();
  const can = { stats: allows(entitlements, "stats"), gate: allows(entitlements, "gateBoard") };
  const orgId = useEnvironmentOrganization();
  const organization = useOrganizations((state) => (orgId ? state.list.find((item) => item.id === orgId) ?? null : null));
  // A aba da organização aberta só acende o item quando é a do ambiente.
  const orgTab = useOrganizations((state) => (view === "organization" && state.openId === orgId ? state.tab : null));
  const { data, activeProjectId, activeChatId } = useWorkspace();
  const project = findProject(data, activeProjectId);
  // O chat só aparece selecionado enquanto a conversa dele está na tela: na
  // grade de chats nenhuma linha fica marcada.
  const openId = view === "chat" ? activeChatId : null;
  const chats = project ? recentChats(chatsOf(data, project.id), openId) : [];
  // Os chats do projeto geral da organização já estão na coluna dos chats
  // gerais, ao lado: a lateral não os repete.
  const generalColumn = useGeneralChatsColumn();
  const listChats = !(project && generalColumn && isGeneralProject(project));

  return (
    <aside className="flex h-full flex-col overflow-hidden border-e border-sidebar-border bg-sidebar px-3 pt-4 pb-3 text-sidebar-foreground">
      <div className="flex items-center gap-2.5 px-1.5 pb-5">
        <BrandMark />
        <div className="flex min-w-0 flex-1 flex-col">
          <strong className="text-sm font-semibold">JayV</strong>
          <small className="truncate text-caption text-sidebar-muted">{t("brand.tagline")}</small>
        </div>
        {/* O sino e a ajuda ficam no alto, à direita do logotipo. */}
        <div className="flex shrink-0 items-center self-start">
          <NotificationBell />
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            aria-label={t("tutorial.help")}
            data-tour="help-tutorial"
            onClick={() => (tourForView(view) ? startTourHere(view) : startTour(TOURS[0].id))}
            className="grid size-9 shrink-0 place-items-center rounded-md text-sidebar-muted transition-colors outline-none hover:bg-sidebar-accent/70 hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-ring"
          >
            <CircleHelpIcon aria-hidden="true" className="size-[18px]" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom">{t("tutorial.help")}</TooltipContent>
      </Tooltip>
        </div>
      </div>

      <EnvironmentSwitch />

      {!project && orgId ? (
        <div className="flex min-h-0 flex-col">
          <OrganizationPlate organization={organization} />
          <nav className="grid gap-0.5">
            {/* Os projetos são a tela de projetos do ambiente, que já só tem os
                da organização; as outras abas abrem a página dela. */}
            <NavItem active={view === "projects"} mark={<FolderKanbanIcon />} shortcut="projects" onClick={() => navigate("projects")}>{t("nav.projects")}</NavItem>
            {ORGANIZATION_TABS.filter(({ feature }) => !feature || allows(entitlements, feature)).map(({ tab, label }) => {
              const Icon = TAB_ICONS[tab];
              return (
                <NavItem key={tab} data-tour={tab === "stats" ? "nav-stats" : undefined} active={orgTab === tab} mark={<Icon />} shortcut={tab === "stats" ? "stats" : undefined}
                  onClick={() => void openOrganization(orgId, tab).catch(reportError)}>
                  {t(label)}
                </NavItem>
              );
            })}
          </nav>
        </div>
      ) : !project ? (
        <nav className="grid gap-0.5">
          <NavItem active={view === "projects"} mark={<FolderKanbanIcon />} shortcut="projects" onClick={() => navigate("projects")}>{t("nav.projects")}</NavItem>
          {can.stats && <NavItem data-tour="nav-stats" active={view === "stats"} mark={<ChartColumnIcon />} shortcut="stats" onClick={() => openStats({ kind: "global" })}>{t("nav.stats")}</NavItem>}
          <NavItem active={view === "status"} mark={<ActivityIcon />} shortcut="system" onClick={() => navigate("status")}>{t("nav.system")}</NavItem>
          <NavItem active={view === "plans"} mark={<CreditCardIcon />} onClick={() => navigate("plans")}>{t("nav.plans")}</NavItem>
        </nav>
      ) : (
        <div className="flex min-h-0 flex-col">
          <button
            type="button"
            onClick={leaveProject}
            className="mb-2 flex h-8 w-full items-center gap-2 rounded-md px-2.5 text-start text-xs font-medium text-sidebar-muted transition-colors outline-none hover:bg-sidebar-accent/70 hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-ring"
          >
            <BackMark className="w-4 text-center" />
            {t("nav.allProjects")}
          </button>
          <ProjectPlate project={project} />
          <NavItem active={view === "chats"} mark={<MessagesSquareIcon />} onClick={() => navigate("chats")}>{t("nav.chats")}</NavItem>
          {can.gate && <NavItem
            data-tour="nav-gate"
            active={view === "gate"}
            className="mb-1"
            shortcut="gate"
            mark={<LogoIcon className="size-[18px] [&_.logo-halo]:stroke-ask [&_.logo-lamp]:fill-ask" />}
            onClick={() => navigate("gate")}
          >
            {t("nav.gate")}
          </NavItem>}
          {can.stats && <NavItem active={view === "stats"} mark={<ChartColumnIcon />} onClick={() => openStats({ kind: "project", id: project.id })}>{t("nav.stats")}</NavItem>}
          {listChats && <>
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
          </>}
        </div>
      )}

      {/* A conta abre o perfil e a engrenagem, as
          configurações (o sino e a ajuda ficam no alto) — o idioma mora lá dentro. */}
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
            <UserAvatar name={name} src={photo ?? profile?.avatarUrl} className="size-7 text-xs" />
            <span className="grid min-w-0">
              <span className="truncate text-[13px] font-medium text-sidebar-foreground">{name}</span>
              {email && name !== email && <span className="truncate text-caption text-sidebar-muted">{email}</span>}
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
