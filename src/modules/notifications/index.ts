import { create } from "zustand";
import { toast } from "sonner";
import type { RealtimeChannel } from "@supabase/supabase-js";
import { bus, type Chat } from "@/modules/core";
import { onCore } from "@/modules/core/bridge";
import { supabase } from "@/modules/auth/client";
import { t, type Key } from "@/modules/i18n";
import { navigate, useNavigation } from "@/modules/navigation";
import { loadOrganizations, openOrganization, useOrganizations } from "@/modules/organizations";
import { agentLabel, openStats, windowLabel } from "@/modules/usage";
import { chatTitle, findProject, openChat, useWorkspace } from "@/modules/workspace";
import { emptyMemory, LOCAL_LIMIT, newestFirst, targetOf, turnEvents, type AppNotification, type NotificationData, type NotificationKind } from "./rules";

export * from "./rules";

interface NotificationsState {
  /** As da conta, do banco. */
  account: AppNotification[];
  /** As deste aparelho, só na memória. */
  device: AppNotification[];
}

export const useNotifications = create<NotificationsState>(() => ({ account: [], device: [] }));

export function allNotifications(state: NotificationsState) {
  return newestFirst([...state.account, ...state.device]);
}

type Row = { id: string; kind: string; data: NotificationData | null; created_at: string; read_at: string | null };

const fromRow = (row: Row): AppNotification => ({
  id: row.id, kind: row.kind as NotificationKind, data: row.data ?? {}, createdAt: row.created_at, read: row.read_at !== null, local: false,
});

let channel: RealtimeChannel | null = null;
let memory = emptyMemory();
let learned = false;

/** A frase da notificação no idioma atual, e a linha de baixo (projeto ou quem
 * fez), quando há. */
export function describe(notification: AppNotification, say: (key: Key, params?: Record<string, string | number>) => string = t) {
  const data = notification.data;
  const text = (value: unknown) => (typeof value === "string" ? value : "");
  const role = data.role ? say(`org.role.${data.role}` as Key) : "";
  const user = data.user ? `@${data.user}` : say("notifications.someone");
  const by = data.user ? say("notifications.by", { user: `@${data.user}` }) : null;
  const params = { org: text(data.org), role, user, repository: text(data.repository) };
  switch (notification.kind) {
    case "org.invited": return { title: say("notifications.org.invited", params), detail: by };
    case "org.inviteAccepted": return { title: say("notifications.org.inviteAccepted", params), detail: null };
    case "org.inviteDeclined": return { title: say("notifications.org.inviteDeclined", params), detail: null };
    case "org.roleChanged": return { title: say("notifications.org.roleChanged", params), detail: by };
    case "org.removed": return { title: say("notifications.org.removed", params), detail: by };
    case "org.deleted": return { title: say("notifications.org.deleted", params), detail: by };
    case "org.policyChanged": return { title: say(data.repository ? "notifications.org.repositoryPolicyChanged" : "notifications.org.policyChanged", params), detail: by };
    case "quota.crossed": return {
      title: say("notifications.quota.crossed", { agent: agentLabel(text(data.agent)), percent: Number(data.percent ?? 0) }),
      detail: windowLabel(text(data.window), say),
    };
    default: {
      // O título do chat pode ter mudado depois (o núcleo renomeia após a
      // primeira resposta): vale o atual, se o chat ainda existe.
      const { data: workspace } = useWorkspace.getState();
      const chat = workspace.chats.find((item) => item.id === data.chatId);
      const project = findProject(workspace, chat?.projectId ?? text(data.projectId));
      const title = chat ? chatTitle(chat) : text(data.chat) || say("common.newChat");
      return { title: say(`notifications.${notification.kind}` as Key, { chat: title }), detail: project?.name ?? null };
    }
  }
}

/** Leva para onde a notificação aponta e a marca como lida. */
export function openNotification(notification: AppNotification) {
  void markRead(notification);
  const target = targetOf(notification);
  if (!target) return;
  if (target.kind === "chat") openChat(target.chatId);
  else if (target.kind === "stats") openStats({ kind: "global" });
  else if (target.kind === "organization" && useOrganizations.getState().list.some((org) => org.id === target.orgId)) void openOrganization(target.orgId);
  else navigate("organizations");
}

function announce(notification: AppNotification) {
  const { title, detail } = describe(notification);
  toast(title, {
    description: detail ?? undefined,
    duration: 8000,
    action: targetOf(notification) ? { label: t("notifications.open"), onClick: () => openNotification(notification) } : undefined,
  });
}

function addDevice(id: string, kind: NotificationKind, data: NotificationData) {
  if (useNotifications.getState().device.some((item) => item.id === id)) return;
  const notification: AppNotification = { id, kind, data, createdAt: new Date().toISOString(), read: false, local: true };
  useNotifications.setState((state) => ({ device: newestFirst([notification, ...state.device]).slice(0, LOCAL_LIMIT) }));
  announce(notification);
}

function upsertAccount(notification: AppNotification) {
  useNotifications.setState((state) => ({
    account: newestFirst([notification, ...state.account.filter((item) => item.id !== notification.id)]),
  }));
}

/** O chat que está na tela, com a janela em uso: o que acontece nele já está
 * sendo visto e não vira notificação. */
function watching(chat: Chat) {
  return useNavigation.getState().view === "chat"
    && useWorkspace.getState().activeChatId === chat.id
    && document.visibilityState === "visible"
    && document.hasFocus();
}

/** Chamada a cada login (ou troca de conta): começa do zero, antes de a
 * primeira leitura do workspace da conta nova chegar. */
export async function loadNotifications() {
  clearNotifications();
  const { data: session } = await supabase.auth.getUser();
  const me = session.user?.id;
  if (!me) return;
  const { data, error } = await supabase.from("notifications").select("id, kind, data, created_at, read_at").order("created_at", { ascending: false }).limit(100);
  if (error) throw error;
  useNotifications.setState({ account: (data as Row[]).map(fromRow) });
  // As novas chegam ao vivo; a que mexe numa organização relê a lista delas
  // (o convite aparece, a organização excluída some).
  if (channel) void supabase.removeChannel(channel);
  channel = supabase
    .channel(`notifications:${me}`)
    .on("postgres_changes", { event: "INSERT", schema: "public", table: "notifications", filter: `user_id=eq.${me}` }, ({ new: row }) => {
      const notification = fromRow(row as Row);
      upsertAccount(notification);
      announce(notification);
      loadOrganizations().catch((failure) => console.error("organizations", failure));
    })
    .on("postgres_changes", { event: "UPDATE", schema: "public", table: "notifications", filter: `user_id=eq.${me}` }, ({ new: row }) => {
      upsertAccount(fromRow(row as Row));
    })
    .subscribe();
}

export function clearNotifications() {
  if (channel) void supabase.removeChannel(channel);
  channel = null;
  memory = emptyMemory();
  learned = false;
  useNotifications.setState({ account: [], device: [] });
}

async function markAccount(ids: string[] | null) {
  const { error } = await supabase.rpc("mark_notifications_read", { ids });
  if (error) console.error("notifications", error);
}

export async function markRead(notification: AppNotification) {
  if (notification.read) return;
  const read = (item: AppNotification) => (item.id === notification.id ? { ...item, read: true } : item);
  useNotifications.setState((state) => ({ account: state.account.map(read), device: state.device.map(read) }));
  if (!notification.local) await markAccount([notification.id]);
}

export async function markAllRead() {
  const { account } = useNotifications.getState();
  const read = (item: AppNotification) => ({ ...item, read: true });
  useNotifications.setState((state) => ({ account: state.account.map(read), device: state.device.map(read) }));
  if (account.some((item) => !item.read)) await markAccount(null);
}

export async function clearRead() {
  useNotifications.setState((state) => ({ account: state.account.filter((item) => !item.read), device: state.device.filter((item) => !item.read) }));
  const { error } = await supabase.rpc("clear_notifications");
  if (error) console.error("notifications", error);
}

/** Cada leitura do banco local é comparada com a anterior: o turno que fechou
 * e a pergunta nova viram notificação, a não ser no chat que está na tela.
 * A cota que passa de um patamar (80, 95, 100) também. */
export function connectNotifications() {
  const offLoaded = bus.on("workspace:loaded", ({ chats }) => {
    const events = turnEvents(memory, chats, !learned);
    learned = true;
    for (const { kind, chat, turnId } of events) {
      if (watching(chat)) continue;
      addDevice(`${kind}:${turnId}`, kind, { chatId: chat.id, chat: chatTitle(chat), projectId: chat.projectId });
    }
  });
  const offQuota = onCore("quota-changed", ({ quota, crossed }) => {
    if (!crossed) return;
    addDevice(`quota:${quota.agent}:${quota.window}:${crossed}:${quota.resetsAt ?? ""}`, "quota.crossed", { agent: quota.agent, window: quota.window, percent: crossed });
  });
  return () => {
    offLoaded();
    void offQuota.then((unlisten) => unlisten());
  };
}
