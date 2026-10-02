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
export function groupByScope<P extends { id: string }>(
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
    const link = links[project.id];
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
