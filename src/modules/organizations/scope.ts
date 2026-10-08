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

/** A organização do projeto: a do vínculo do servidor ou a que ele mesmo diz. */
export function projectOrgId(project: { id: string; orgId?: string | null }, links: Record<string, { orgId: string }>): string | null {
  return links[project.id]?.orgId ?? project.orgId ?? null;
}

/** O projeto geral da organização é o que junta todos os repositórios dela
 * numa pasta só (tem `orgId`); os projetos de cada repositório são ligados
 * pelo remote e não têm. */
export function isGeneralProject(project: { orgId?: string | null }): boolean {
  return !!project.orgId;
}

/** Separa os projetos de um bloco: o geral da organização (de onde saem os
 * chats gerais) e os de cada repositório. */
export function splitGeneral<P extends { orgId?: string | null }>(projects: P[]): { general: P[]; repositories: P[] } {
  return { general: projects.filter(isGeneralProject), repositories: projects.filter((project) => !isGeneralProject(project)) };
}

/** Os chats gerais da organização (os do projeto que junta os repositórios
 * dela) com pasta neste computador, do mais recente para o mais antigo. */
export function organizationChatsOf<C extends { id: string; projectId: string; updatedAt: string }>(
  data: { projects: { id: string; rootPath: string; orgId?: string | null }[]; chats: C[] },
  orgId: string,
): C[] {
  const projects = new Set(data.projects.filter((project) => project.orgId === orgId && project.rootPath.trim()).map((project) => project.id));
  return data.chats
    .filter((chat) => projects.has(chat.projectId))
    .sort((a, b) => new Date(b.updatedAt).valueOf() - new Date(a.updatedAt).valueOf());
}

/** O chat geral mais recente da organização, ou `null` quando ainda não há. */
export function organizationChatOf<C extends { id: string; projectId: string; updatedAt: string }>(
  data: { projects: { id: string; rootPath: string; orgId?: string | null }[]; chats: C[] },
  orgId: string,
): C | null {
  return organizationChatsOf(data, orgId)[0] ?? null;
}

/** Os blocos que a busca deixa à vista. Ela olha nome, pasta e repositórios do
 * projeto, sem diferenciar maiúsculas; com ela preenchida, os blocos sem nenhum
 * projeto que combine somem. */
export function searchScopeGroups<P extends { name: string; rootPath: string; repoKeys?: string[] }>(groups: ScopeGroup<P>[], query: string): ScopeGroup<P>[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return groups;
  const matches = (project: P) => [project.name, project.rootPath, ...(project.repoKeys ?? [])].some((text) => text.toLocaleLowerCase().includes(needle));
  return groups
    .map((group) => ({ ...group, projects: group.projects.filter(matches) }))
    .filter((group) => group.projects.length > 0);
}
