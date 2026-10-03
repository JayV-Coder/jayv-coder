import type { Role } from "./rules";

/** Onde o projeto mora: só com quem o usa, ou numa organização. */
export type ProjectScope = { kind: "personal" } | { kind: "organization"; orgId: string };

export interface ScopeGroup<P> {
  /** `personal` ou o id da organização. */
  key: string;
  scope: ProjectScope;
  name: string;
  slug: string | null;
  /** O papel de quem usa o app; nulo quando a organização ainda não voltou na lista. */
  role: Role | null;
  projects: P[];
}

/** Separa os projetos em pessoais e por organização. Os pessoais vêm primeiro e
 * sempre aparecem; depois cada organização de quem usa o app, em ordem
 * alfabética, mesmo sem projeto nenhum dela neste computador. Um projeto ligado
 * a uma organização que ainda não voltou na lista ganha o grupo dela com o
 * nome que o vínculo trouxe. */
export function groupByScope<P extends { id: string; orgId?: string | null }>(
  projects: P[],
  links: Record<string, { orgId: string; slug: string; name: string }>,
  organizations: { id: string; name: string; slug: string; role: Role }[],
): ScopeGroup<P>[] {
  const personal: ScopeGroup<P> = { key: "personal", scope: { kind: "personal" }, name: "", slug: null, role: null, projects: [] };
  const groups = new Map<string, ScopeGroup<P>>();
  for (const org of organizations) {
    groups.set(org.id, { key: org.id, scope: { kind: "organization", orgId: org.id }, name: org.name, slug: org.slug, role: org.role, projects: [] });
  }
  for (const project of projects) {
    const link = links[project.id] ?? ownLink(project, organizations);
    if (!link) { personal.projects.push(project); continue; }
    let group = groups.get(link.orgId);
    if (!group) {
      group = { key: link.orgId, scope: { kind: "organization", orgId: link.orgId }, name: link.name, slug: link.slug, role: null, projects: [] };
      groups.set(link.orgId, group);
    }
    group.projects.push(project);
  }
  const named = [...groups.values()].sort((a, b) => a.name.localeCompare(b.name));
  return [personal, ...named];
}

/** O vínculo que o próprio projeto diz (o chat da organização), antes de a
 * sincronização trazer o do servidor. Só vale para uma organização de quem usa
 * o app. */
function ownLink(project: { orgId?: string | null }, organizations: { id: string; name: string; slug: string }[]) {
  const org = project.orgId ? organizations.find((item) => item.id === project.orgId) : undefined;
  return org ? { orgId: org.id, slug: org.slug, name: org.name } : null;
}

/** A organização do projeto: a do vínculo do servidor ou a que ele mesmo diz. */
export function projectOrgId(project: { id: string; orgId?: string | null }, links: Record<string, { orgId: string }>): string | null {
  return links[project.id]?.orgId ?? project.orgId ?? null;
}

/** O chat mais recente do projeto da organização (o que junta os repositórios
 * dela) com pasta neste computador. Quando existe, o botão leva direto a ele
 * em vez de abrir outro. */
export function organizationChatOf<C extends { id: string; projectId: string; updatedAt: string }>(
  data: { projects: { id: string; rootPath: string; orgId?: string | null }[]; chats: C[] },
  orgId: string,
): C | null {
  const projects = new Set(data.projects.filter((project) => project.orgId === orgId && project.rootPath.trim()).map((project) => project.id));
  return data.chats
    .filter((chat) => projects.has(chat.projectId))
    .sort((a, b) => new Date(b.updatedAt).valueOf() - new Date(a.updatedAt).valueOf())[0] ?? null;
}

/** O filtro da lista de projetos: tudo, só os pessoais ou só os das
 * organizações (todas juntas). */
export type ScopeFilter = "all" | "personal" | "organizations";

/** Os blocos que o filtro e a busca deixam à vista. A busca olha nome, pasta e
 * repositórios do projeto, sem diferenciar maiúsculas; com ela preenchida, os
 * blocos sem nenhum projeto que combine somem. */
export function filterScopeGroups<P extends { name: string; rootPath: string; repoKeys?: string[] }>(
  groups: ScopeGroup<P>[],
  filter: ScopeFilter,
  query: string,
): ScopeGroup<P>[] {
  const scoped = groups.filter((group) => filter === "all" || (filter === "personal") === (group.scope.kind === "personal"));
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return scoped;
  const matches = (project: P) => [project.name, project.rootPath, ...(project.repoKeys ?? [])].some((text) => text.toLocaleLowerCase().includes(needle));
  return scoped
    .map((group) => ({ ...group, projects: group.projects.filter(matches) }))
    .filter((group) => group.projects.length > 0);
}
