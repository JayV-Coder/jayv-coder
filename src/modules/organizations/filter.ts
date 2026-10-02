import type { UsageScope } from "@/modules/core";

/** O recorte das abas Estatísticas e Portaria da organização: todos os
 * projetos dela, um projeto, ou um chat de um projeto. As duas abas dividem o
 * mesmo recorte, então trocar de aba não perde o filtro. */
export interface DashboardFilter { projectId: string | null; chatId: string | null }

/** A conta que as estatísticas pedem ao núcleo. Um projeto ou chat que não é
 * da organização não alarga a vista: cai nos projetos dela. */
export function dashboardScope(filter: DashboardFilter, projectIds: string[]): UsageScope {
  if (!filter.projectId || !projectIds.includes(filter.projectId)) return { kind: "projects", id: projectIds };
  if (filter.chatId) return { kind: "chat", id: filter.chatId };
  return { kind: "project", id: filter.projectId };
}

/** Os projetos que a portaria lê e o chat que recorta, pelo mesmo critério. */
export function dashboardGateScope(filter: DashboardFilter, projectIds: string[]): { projectIds: string[]; chatId: string | null } {
  if (!filter.projectId || !projectIds.includes(filter.projectId)) return { projectIds, chatId: null };
  return { projectIds: [filter.projectId], chatId: filter.chatId };
}
