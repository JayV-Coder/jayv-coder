import { create } from "zustand";
import { supabase } from "@/modules/auth/client";
import type { Provider } from "@/modules/auth";
import { navigate, useNavigation } from "@/modules/navigation";

export * from "./rules";
export * from "./policy";
export * from "./scope";
export * from "./checkout";
export * from "./local";
export * from "./filter";
export * from "./dashboard";
export * from "./repositories";

import type { Role } from "./rules";
import { connectDashboard } from "./dashboard";
import { storedPolicy, type StoredPolicy } from "./policy";

export interface Organization {
  id: string;
  name: string;
  slug: string;
  /** O papel de quem está usando o app. */
  role: Role;
  members: number;
  repositories: number;
}

export interface Member { userId: string; username: string; displayName: string; avatarUrl: string | null; role: Role; joinedAt: string }
export interface Repository { id: string; provider: Provider; path: string; repoKey: string }
export interface IncomingInvite { id: string; orgId: string; orgName: string; orgSlug: string; role: Role; invitedBy: string | null; expiresAt: string }
export interface ProjectOrganization { orgId: string; slug: string; name: string }

export interface OrganizationDetail {
  id: string;
  members: Member[];
  repositories: Repository[];
  /** A política da organização e as dos repositórios dela. */
  policies: StoredPolicy[];
}

export type OrganizationTab = "projects" | "stats" | "gate" | "members" | "repositories";

interface OrganizationsState {
  list: Organization[];
  incoming: IncomingInvite[];
  /** A organização de cada projeto associado, por id do projeto. */
  projects: Record<string, ProjectOrganization>;
  /** Os projetos que rodam sob uma política de LLM, por id. */
  policed: Record<string, true>;
  /** Os mecanismos (`agente/mecanismo`) que alguma organização bloqueia nos
   * projetos dela, com as organizações que bloqueiam. */
  blockedMechanisms: Record<string, string[]>;
  loaded: boolean;
  /** A organização aberta na vista `organization`. */
  openId: string | null;
  /** A aba aberta da organização. */
  tab: OrganizationTab;
  detail: OrganizationDetail | null;
}

export const useOrganizations = create<OrganizationsState>(() => ({ list: [], incoming: [], projects: {}, policed: {}, blockedMechanisms: {}, loaded: false, openId: null, tab: "projects", detail: null }));

/** As RPCs falham com uma chave do i18n (`org.forbidden`); o resto segue como
 * veio. */
function failure(error: { message?: string; code?: string }) {
  if (error.message && /^(org|policy)\.[A-Za-z]+$/.test(error.message)) return { key: error.message };
  return error;
}

async function call<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  const { data, error } = await supabase.rpc(name, args);
  if (error) throw failure(error);
  return data as T;
}

async function userId() {
  const { data } = await supabase.auth.getUser();
  if (!data.user) throw new Error("no session");
  return data.user.id;
}

const count = (rows: { org_id: string }[] | null) => {
  const tally: Record<string, number> = {};
  for (const row of rows ?? []) tally[row.org_id] = (tally[row.org_id] ?? 0) + 1;
  return tally;
};

type Row = Record<string, unknown>;

/** Quem bloqueia cada mecanismo, a partir das políticas dos projetos. */
function blockedBy(rows: Row[]) {
  const found: Record<string, string[]> = {};
  for (const row of rows) {
    const listed = (row.policy as { blocked_mechanisms?: unknown } | null)?.blocked_mechanisms;
    const keys = Array.isArray(listed) ? listed.map(String) : [];
    for (const slug of String(row.org_slug ?? "").split(", ").filter(Boolean)) {
      for (const key of keys) if (!(found[key] ??= []).includes(slug)) found[key].push(slug);
    }
  }
  return found;
}

export async function loadOrganizations() {
  const me = await userId();
  const [mine, members, repositories, incoming, projects, policed] = await Promise.all([
    supabase.from("organization_members").select("role, organizations(id, name, slug)").eq("user_id", me),
    supabase.from("organization_members").select("org_id"),
    supabase.from("organization_repositories").select("org_id"),
    call<Row[]>("my_invites"),
    call<Row[]>("my_project_organizations"),
    // Antes da migração da política, o resto da tela continua de pé.
    call<Row[]>("my_project_policies").catch(() => [] as Row[]),
  ]);
  for (const result of [mine, members, repositories]) if (result.error) throw failure(result.error);
  const memberCount = count(members.data);
  const repositoryCount = count(repositories.data);
  const list = (mine.data ?? []).flatMap((row) => {
    const org = row.organizations as unknown as { id: string; name: string; slug: string } | null;
    return org ? [{ ...org, role: row.role as Role, members: memberCount[org.id] ?? 0, repositories: repositoryCount[org.id] ?? 0 }] : [];
  }).sort((a, b) => a.name.localeCompare(b.name));
  useOrganizations.setState({
    list,
    loaded: true,
    incoming: (incoming ?? []).map((row) => ({
      id: row.id as string, orgId: row.org_id as string, orgName: row.org_name as string, orgSlug: row.org_slug as string,
      role: row.role as Role, invitedBy: (row.invited_by_username as string) ?? null, expiresAt: row.expires_at as string,
    })),
    projects: Object.fromEntries((projects ?? []).map((row) => [row.project_id as string, { orgId: row.org_id as string, slug: row.org_slug as string, name: row.org_name as string }])),
    policed: Object.fromEntries((policed ?? []).map((row) => [row.project_id as string, true as const])),
    blockedMechanisms: blockedBy(policed ?? []),
  });
}

export async function loadDetail(id: string) {
  const [members, repositories, policies] = await Promise.all([
    call<Row[]>("organization_members_view", { org: id }),
    supabase.from("organization_repositories").select("id, provider, path, repo_key").eq("org_id", id).order("repo_key"),
    supabase.from("organization_llm_policies").select("*").eq("org_id", id),
  ]);
  if (repositories.error) throw failure(repositories.error);
  if (useOrganizations.getState().openId !== id) return;
  useOrganizations.setState({
    detail: {
      id,
      members: (members ?? []).map((row) => ({
        userId: row.user_id as string, username: row.username as string, displayName: row.display_name as string,
        avatarUrl: (row.avatar_url as string) ?? null, role: row.role as Role, joinedAt: row.joined_at as string,
      })),
      repositories: (repositories.data ?? []).map((row) => ({ id: row.id, provider: row.provider as Provider, path: row.path, repoKey: row.repo_key })),
      // Sem a tabela (migração ainda não aplicada), a aba mostra sem política.
      policies: policies.error ? [] : (policies.data ?? []).map((row) => storedPolicy(row as Row)),
    },
  });
}

/** Os repositórios da organização, para quem abre o chat dela fora da página
 * da organização (a lista de projetos), onde o detalhe não está carregado. */
export async function organizationRepositories(id: string): Promise<Repository[]> {
  const { data, error } = await supabase.from("organization_repositories").select("id, provider, path, repo_key").eq("org_id", id).order("repo_key");
  if (error) throw failure(error);
  return (data ?? []).map((row) => ({ id: row.id, provider: row.provider as Provider, path: row.path, repoKey: row.repo_key }));
}

/** Os repositórios da organização e as políticas dela, para o painel do chat
 * da organização. Sem rede (ou sem a migração), volta vazio: o painel mostra
 * só o que o git diz. */
export async function organizationRules(orgId: string): Promise<{ repositories: Repository[]; policies: StoredPolicy[] }> {
  try {
    const [repositories, policies] = await Promise.all([
      organizationRepositories(orgId),
      supabase.from("organization_llm_policies").select("*").eq("org_id", orgId)
        .then(({ data, error }) => (error ? [] : (data ?? []).map((row) => storedPolicy(row as Row)))),
    ]);
    return { repositories, policies };
  } catch {
    return { repositories: [], policies: [] };
  }
}

export function openOrganization(id: string, tab: OrganizationTab = "projects") {
  useOrganizations.setState({ openId: id, tab, detail: null });
  navigate("organization");
  return loadDetail(id);
}

/** Depois de cada mudança, a lista e a organização aberta voltam do banco. */
async function refresh() {
  await loadOrganizations();
  const { openId, list } = useOrganizations.getState();
  if (!openId) return;
  if (list.some((org) => org.id === openId)) await loadDetail(openId);
  else {
    // Saiu, foi removido ou a organização foi excluída (pelo site): a vista
    // volta para a lista.
    useOrganizations.setState({ openId: null, detail: null });
    navigate("organizations");
  }
}

export async function setMemberRole(org: string, member: string, role: Role) { await call("set_member_role", { org, member, role }); await refresh(); }
export async function removeMember(org: string, member: string) { await call("remove_member", { org, member }); await refresh(); }
export async function addRepository(org: string, provider: Provider, path: string) { await call("add_repository", { org, provider, path }); await refresh(); }
export async function removeRepository(repository: string) { await call("remove_repository", { repository }); await refresh(); }
export async function acceptInvite(invite: string) { await call("accept_invite", { invite }); await refresh(); }
export async function declineInvite(invite: string) { await call("decline_invite", { invite }); await refresh(); }

export function clearOrganizations() {
  useOrganizations.setState({ list: [], incoming: [], projects: {}, policed: {}, blockedMechanisms: {}, loaded: false, openId: null, tab: "projects", detail: null });
}

export function setOrganizationTab(tab: OrganizationTab) {
  useOrganizations.setState({ tab });
}


/** Relê as abas Estatísticas e Portaria da organização aberta quando entra
 * gasto ou pedido novo. */
export function connectOrganizationDashboard() {
  return connectDashboard(() => {
    if (useNavigation.getState().view !== "organization") return null;
    const { tab } = useOrganizations.getState();
    return tab === "stats" || tab === "gate" ? tab : null;
  });
}
