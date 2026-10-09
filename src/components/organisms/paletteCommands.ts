import type { Chat, Project, View, WorkspaceData } from "@/modules/core";
import { WORK_MODES } from "@/modules/core";
import { signOut } from "@/modules/auth";
import { showChanges } from "@/modules/changelog";
import { featureActions, requestIntent, type Shortcut } from "@/modules/commands";
import { cancelTurn, openChatFind } from "@/modules/conversation";
import { reportError } from "@/modules/feedback";
import { setLocale, type Key, type LocaleOption } from "@/modules/i18n";
import { environmentOptions, organizationOf, switchEnvironment } from "@/modules/environments";
import { setLivePanel, useLive } from "@/modules/live";
import { navigate } from "@/modules/navigation";
import { clearRead, markAllRead } from "@/modules/notifications";
import {
  acceptInvite, declineInvite, openOrganization, ORGANIZATION_TABS, organizationChatsOf, type IncomingInvite, type Organization,
} from "@/modules/organizations";
import { allows, SETTINGS_TAB_FEATURE } from "@/modules/plans";
import { AGENT_LABELS, AGENTS, checkGateway, discardChanges, openSettingsTab, restoreCoreDefaults, saveSettings, updateOptions, useSettings, type SettingsTab } from "@/modules/settings";
import { orgExtensionsPath } from "@/modules/orgExtensions";
import { openDashboard, openSite, SITE_URL } from "@/modules/site";
import { setThemePreference, THEME_PREFERENCES } from "@/modules/theme";
import { resetTours, setAutoTours, startTour, startTourHere, tourForView, TOURS, useTutorial } from "@/modules/tutorial";
import { checkForUpdate, setInstallOnLaunch, useUpdate } from "@/modules/updates";
import { openStats, setPeriod, type Period } from "@/modules/usage";
import {
  chatsOf, chatTitle, createChat, findChat, leaveProject, openChat, openProject, openTurns, recentChats, setLayout, setWorkMode, type Layout,
} from "@/modules/workspace";

export interface Command {
  id: string;
  group: Key;
  label: string;
  /** Texto apagado ao lado do nome: o caminho do projeto, por exemplo. */
  hint?: string;
  shortcut?: Shortcut;
  run: () => void;
}

/** O que a paleta precisa saber do app para listar o que dá para fazer agora. */
export interface PaletteContext {
  t: (key: Key, params?: Record<string, string | number>) => string;
  data: WorkspaceData;
  project: Project | null;
  activeChatId: string | null;
  view: View;
  layout: Layout;
  locale: string;
  locales: LocaleOption[];
  organizations: Organization[];
  invites: IncomingInvite[];
  openOrganizationId: string | null;
  /** O ambiente aberto: `personal` ou o id de uma organização. */
  environment: string;
  settingsDirty: boolean;
  rights: Parameters<typeof allows>[0];
}

type PlanFeature = Parameters<typeof allows>[1];

const SETTINGS_TABS: { tab: SettingsTab; label: (t: PaletteContext["t"]) => string }[] = [
  { tab: "app", label: (t) => t("settings.tab.app") },
  { tab: "jev", label: (t) => t("settings.tab.jev") },
  { tab: "mcp", label: (t) => t("settings.tab.mcp") },
  { tab: "skills", label: (t) => t("settings.tab.skills") },
  ...AGENTS.map((id) => ({ tab: id as SettingsTab, label: () => AGENT_LABELS[id] })),
];

const PERIODS: Period[] = ["today", "7d", "30d", "all"];
const SEP = " › ";

/** Todos os comandos da paleta (Ctrl+K), na ordem dos grupos: tudo que tem
 * botão numa tela também tem nome aqui. O que o plano desliga some, como no
 * menu; o que destrói dados (limpar ou apagar chat e projeto) fica só nos
 * botões, que pedem confirmação. No ambiente de uma organização, como no menu
 * lateral, só aparece o que é dela: sem Organizações, Sistema e Planos, e as
 * Estatísticas são as da organização. */
export function paletteCommands(ctx: PaletteContext): Command[] {
  const { t, data, project, view, rights } = ctx;
  const can = (feature: PlanFeature) => allows(rights, feature);
  const orgId = organizationOf(ctx.environment);
  const all: Command[] = [];
  const chat: Chat | null = findChat(data, ctx.activeChatId);

  if (project) {
    const group = "palette.group.project" as const;
    all.push(
      { id: "new-chat", group, label: t("common.newChat"), hint: project.name, shortcut: "newChat", run: () => void createChat(project.id) },
      { id: "chats", group, label: t("nav.chats"), hint: project.name, run: () => navigate("chats") },
    );
    if (can("gateBoard")) all.push({ id: "gate", group, label: t("nav.gate"), hint: project.name, shortcut: "gate", run: () => navigate("gate") });
    if (can("stats")) all.push({ id: "project-stats", group, label: t("nav.stats"), hint: project.name, run: () => openStats({ kind: "project", id: project.id }) });
    if (can("projectNotes")) all.push({ id: "project-notes", group, label: t("memory.open"), hint: project.name, run: () => { navigate("chats"); requestIntent("projectNotes"); } });
    if (can("chatSearch")) all.push({ id: "search-chats", group, label: t("search.label"), hint: project.name, run: () => { navigate("chats"); requestIntent("searchChats"); } });
    if (chat) {
      for (const mode of WORK_MODES) {
        if (mode === (chat.workMode ?? "auto")) continue;
        all.push({ id: `mode-${mode}`, group, label: t("palette.mode", { name: t(`mode.${mode}`) }), hint: t(`mode.${mode}.hint`), run: () => void setWorkMode(chat.id, mode) });
      }
      if (can("liveFiles") && project.rootPath.trim()) {
        all.push({
          id: "live", group, label: t("palette.live"),
          run: () => { openChat(chat.id); setLivePanel(chat.id, useLive.getState().panel[chat.id] !== true); },
        });
      }
      if (can("conversationFind")) all.push({ id: "find-in-chat", group, label: t("find.label"), shortcut: "find", run: () => { openChat(chat.id); openChatFind(); } });
      all.push({ id: "grants", group, label: t("palette.commandPermissions"), run: () => { openChat(chat.id); requestIntent("commandPermissions"); } });
      const flying = openTurns(chat).find((turn) => turn.status === "flying");
      if (flying) all.push({ id: "stop", group, label: t("palette.stop"), run: () => void cancelTurn(flying.id, chat.id) });
    }
    for (const recent of recentChats(chatsOf(data, project.id), ctx.activeChatId)) {
      all.push({ id: `chat-${recent.id}`, group: "palette.group.chats", label: chatTitle(recent), run: () => openChat(recent.id) });
    }
  }

  const go = "palette.group.navigate" as const;
  all.push({ id: "projects", group: go, label: t("nav.projects"), shortcut: "projects", run: leaveProject });
  if (can("organizations") && !orgId) all.push({ id: "organizations", group: go, label: t("nav.organizations"), shortcut: "organizations", run: () => navigate("organizations") });
  // Cada ambiente, menos o que está aberto.
  if (can("organizations")) {
    for (const item of environmentOptions(ctx.organizations)) {
      if (item.id === ctx.environment || ctx.organizations.length === 0) continue;
      all.push({ id: `environment-${item.id}`, group: go, label: t("palette.environment", { name: item.kind === "personal" ? t("environment.personal") : item.name ?? item.id }), run: () => void switchEnvironment(item.id) });
    }
  }
  if (can("stats")) {
    all.push({
      id: "stats", group: go, label: t("nav.stats"), shortcut: "stats",
      run: orgId ? () => void openOrganization(orgId, "stats").catch(reportError) : () => openStats({ kind: "global" }),
    });
  }
  if (!orgId) all.push({ id: "system", group: go, label: t("nav.system"), shortcut: "system", run: () => navigate("status") });
  all.push(
    { id: "settings", group: go, label: t("nav.settings"), shortcut: "settings", run: () => navigate("settings") },
    { id: "profile", group: go, label: t("nav.profile"), run: () => navigate("profile") },
  );
  if (!orgId) all.push({ id: "plans", group: go, label: t("nav.plans"), run: () => navigate("plans") });

  const projects = "palette.group.projects" as const;
  all.push({ id: "new-project", group: projects, label: t("projects.new"), run: () => { leaveProject(); requestIntent("newProject"); } });
  for (const layout of ["grid", "list"] as Layout[]) {
    if (layout === ctx.layout) continue;
    all.push({ id: `layout-${layout}`, group: projects, label: `${t("layout.label")}: ${t(`layout.${layout}`)}`, run: () => { leaveProject(); setLayout(layout); } });
  }
  for (const item of data.projects) {
    if (item.id === project?.id) continue;
    all.push({ id: `project-${item.id}`, group: projects, label: item.name, hint: item.rootPath ?? undefined, run: () => openProject(item.id) });
  }

  const settings = "palette.group.settings" as const;
  for (const { tab, label } of SETTINGS_TABS) {
    const tabFeature = SETTINGS_TAB_FEATURE[tab];
    if (tabFeature && !can(tabFeature)) continue;
    all.push({ id: `settings-${tab}`, group: settings, label: `${t("nav.settings")}${SEP}${label(t)}`, run: () => openSettingsTab(tab) });
  }
  if (ctx.settingsDirty) {
    all.push(
      { id: "settings-save", group: settings, label: t("settings.save"), run: () => void saveSettings() },
      { id: "settings-discard", group: settings, label: t("settings.discard"), run: discardChanges },
    );
  }
  // O "Aprovar servidores MCP" de cada agente: abre a aba dele e vira a chave,
  // e o Salvar da página (ou o comando dele) grava.
  for (const id of AGENTS) {
    const tabFeature = SETTINGS_TAB_FEATURE[id];
    if (!can("mcp") || (tabFeature && !can(tabFeature))) continue;
    const on = useSettings.getState().agents.find((agent) => agent.id === id)?.options as { approveMcps?: boolean } | undefined;
    all.push({
      id: `mcp-approve-${id}`, group: settings,
      label: t(on?.approveMcps ? "palette.mcpRevoke" : "palette.mcpApprove", { name: AGENT_LABELS[id] }),
      run: () => { openSettingsTab(id); updateOptions(id, { approveMcps: !on?.approveMcps } as never); },
    });
  }
  // Conferir o endereço e a chave dos gateways de API (o botão da aba deles).
  for (const id of ["openrouter", "litellm"] as const) {
    if (can("gatewayProviders")) all.push({ id: `gateway-check-${id}`, group: settings, label: t("gateway.check.named", { name: AGENT_LABELS[id] }), run: () => { openSettingsTab(id); void checkGateway(id); } });
  }
  if (can("skills") && can("skillsHub")) all.push({ id: "skills-hub", group: settings, label: t("skills.hub.title"), run: () => { openSettingsTab("skills"); requestIntent("searchSkillHub"); } });
  if (can("skills")) all.push({ id: "skills-install", group: settings, label: t("skills.install"), run: () => { openSettingsTab("skills"); requestIntent("installSkill"); } });
  all.push({ id: "settings-defaults", group: settings, label: t("settings.defaults"), run: () => { navigate("settings"); restoreCoreDefaults(); } });

  if (view === "stats" && can("stats")) {
    for (const period of PERIODS) {
      all.push({ id: `period-${period}`, group: "palette.group.stats", label: `${t("nav.stats")}${SEP}${t(`usage.period.${period}`)}`, run: () => setPeriod(period) });
    }
  }

  if (can("organizations")) {
    // No ambiente de uma organização, só ela; as outras se abrem trocando de ambiente.
    const shown = orgId ? ctx.organizations.filter((organization) => organization.id === orgId) : ctx.organizations;
    for (const organization of shown) {
      all.push({ id: `org-${organization.id}`, group: "palette.group.organizations", label: organization.name, run: () => void openOrganization(organization.id).catch(reportError) });
      // O que a organização dá aos membros (servidores MCP e skills) se cadastra no site.
      if (SITE_URL) {
        all.push(
          { id: `org-site-mcp-${organization.id}`, group: "palette.group.organizations", label: t("palette.orgSiteMcp", { org: organization.name }), run: () => void openDashboard(orgExtensionsPath(organization.id, "mcp")).catch(reportError) },
          { id: `org-site-permissions-${organization.id}`, group: "palette.group.organizations", label: t("palette.orgSitePermissions", { org: organization.name }), run: () => void openDashboard(orgExtensionsPath(organization.id, "permissions")).catch(reportError) },
          { id: `org-site-skills-${organization.id}`, group: "palette.group.organizations", label: t("palette.orgSiteSkills", { org: organization.name }), run: () => void openDashboard(orgExtensionsPath(organization.id, "skills")).catch(reportError) },
        );
      }
      const general = organizationChatsOf(data, organization.id);
      for (const item of general.slice(0, 5)) {
        all.push({ id: `general-${item.id}`, group: "palette.group.general", label: chatTitle(item), hint: organization.name, run: () => openChat(item.id) });
      }
      if (general[0]) {
        all.push({ id: `general-new-${organization.id}`, group: "palette.group.general", label: t("palette.generalNew", { org: organization.name }), run: () => void createChat(general[0].projectId) });
      }
    }
    // As abas da organização aberta ou, no ambiente dela, sempre as dela (os
    // projetos já são o "Projetos" de Navegar).
    const current = ctx.organizations.find((organization) => organization.id === (orgId ?? ctx.openOrganizationId));
    if (current) {
      for (const { tab, label, feature } of ORGANIZATION_TABS) {
        if ((feature && !can(feature)) || (orgId && tab === "projects")) continue;
        all.push({ id: `org-tab-${tab}`, group: "palette.group.organization", label: `${current.name}${SEP}${t(label)}`, run: () => void openOrganization(current.id, tab).catch(reportError) });
      }
    }
    for (const invite of ctx.invites) {
      all.push(
        { id: `invite-accept-${invite.id}`, group: "palette.group.organizations", label: `${t("org.invites.accept")}${SEP}${invite.orgName}`, run: () => void acceptInvite(invite.id).catch(reportError) },
        { id: `invite-decline-${invite.id}`, group: "palette.group.organizations", label: `${t("org.invites.decline")}${SEP}${invite.orgName}`, run: () => void declineInvite(invite.id).catch(reportError) },
      );
    }
  }

  for (const preference of THEME_PREFERENCES) {
    all.push({ id: `theme-${preference}`, group: "palette.group.appearance", label: t("palette.theme", { name: t(`theme.${preference}`) }), run: () => setThemePreference(preference) });
  }
  for (const option of ctx.locales) {
    if (option.id === ctx.locale) continue;
    all.push({ id: `language-${option.id}`, group: "palette.group.language", label: t("palette.language", { name: option.name }), run: () => setLocale(option.id) });
  }

  const account = "palette.group.account" as const;
  all.push(
    { id: "notifications", group: account, label: t("notifications.title"), run: () => requestIntent("notifications") },
    { id: "notifications-read", group: account, label: t("notifications.markAllRead"), run: () => void markAllRead().catch(reportError) },
    { id: "notifications-clear", group: account, label: t("notifications.clearRead"), run: () => void clearRead().catch(reportError) },
    { id: "sign-out", group: account, label: t("auth.signOut"), run: () => void signOut() },
  );

  const help = "palette.group.help" as const;
  all.push(
    { id: "whats-new", group: help, label: t("palette.whatsNew"), run: () => void showChanges() },
    { id: "check-update", group: help, label: t("system.update.check"), run: () => void checkForUpdate(true) },
    { id: "update-on-launch", group: help, label: t("app.updates.launch"), run: () => setInstallOnLaunch(!useUpdate.getState().installOnLaunch) },
    ...(tourForView(ctx.view) ? [{ id: "tutorial-here", group: help, label: t("tutorial.here"), run: () => startTourHere(ctx.view) }] : []),
    ...TOURS.map((tour) => ({ id: `tutorial-${tour.id}`, group: help, label: t("tutorial.palette", { name: t(`tutorial.tour.${tour.id}` as never) }), run: () => startTour(tour.id) })),
    { id: "tutorial-auto", group: help, label: t("tutorial.auto"), run: () => setAutoTours(!useTutorial.getState().auto) },
    { id: "tutorial-reset", group: help, label: t("tutorial.reset"), run: () => resetTours() },
    { id: "system-copy", group: help, label: t("system.copy"), run: () => void featureActions.copySystemReport?.().catch(reportError) },
  );
  // Reler o sistema abre a tela Sistema, que só existe no ambiente pessoal.
  if (!orgId) all.push({ id: "system-reload", group: help, label: t("system.reload"), run: () => { navigate("status"); featureActions.reloadSystem?.(); } });
  if (SITE_URL) {
    all.push(
      { id: "site-docs", group: help, label: t("palette.docs"), run: () => void openSite("/docs").catch(reportError) },
      { id: "site-releases", group: help, label: t("palette.releases"), run: () => void openSite("/releases").catch(reportError) },
      { id: "site-dashboard", group: help, label: t("org.site.manage"), run: () => void openDashboard().catch(reportError) },
      { id: "site-account", group: help, label: t("palette.siteAccount"), run: () => void openDashboard("/account").catch(reportError) },
    );
  }
  return all;
}
