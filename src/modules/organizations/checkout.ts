import type { FoundRepository, Project } from "@/modules/core";

/** Um caminho comparável: sem espaço nas pontas e sem barra no fim. */
const pathKey = (path: string) => path.trim().replace(/[\\/]+$/, "");

/** O projeto deste computador que é um clone do repositório. Projeto sem pasta
 * (o que veio de outro computador pela sincronização) não conta. */
export function localCopy<P extends Pick<Project, "rootPath" | "repoKeys">>(repoKey: string, projects: P[]): P | null {
  return projects.find((project) => project.rootPath.trim() && (project.repoKeys ?? []).includes(repoKey)) ?? null;
}

/** Dos clones achados na pasta, os que ainda não são projeto: nem a pasta é de
 * um projeto, nem o repositório já está neste computador em outra pasta. */
export function newClones(found: FoundRepository[], projects: Pick<Project, "rootPath" | "repoKeys">[]): FoundRepository[] {
  const folders = new Set(projects.map((project) => pathKey(project.rootPath)).filter(Boolean));
  return found.filter((clone) => !folders.has(pathKey(clone.path)) && !localCopy(clone.key, projects));
}

const FOLDER_KEY = "jayv.organizationFolder.";

/** A pasta onde ficam os repositórios da organização neste computador. Fica só
 * neste computador: cada máquina tem as suas pastas. */
export function organizationFolder(orgId: string): string | null {
  try {
    return localStorage.getItem(FOLDER_KEY + orgId) || null;
  } catch {
    return null;
  }
}

export function rememberOrganizationFolder(orgId: string, path: string) {
  try {
    localStorage.setItem(FOLDER_KEY + orgId, path);
  } catch {
    // Sem armazenamento, a pasta é perguntada de novo na próxima vez.
  }
}

/** Os repositórios da organização que o chat dela alcança. O chat trabalha com
 * a pasta da organização como raiz: entra o clone que está dentro dela; o que
 * está neste computador em outra pasta fica de fora, e o que não está aqui
 * nem conta. */
export interface ChatReach<R> { inside: { repository: R; path: string }[]; elsewhere: { repository: R; path: string }[]; missing: R[] }

const comparable = (path: string) => pathKey(path).replace(/\\/g, "/");

export function chatReach<R extends { repoKey: string }>(
  repositories: R[],
  projects: Pick<Project, "rootPath" | "repoKeys">[],
  folder: string,
): ChatReach<R> {
  const base = comparable(folder);
  const reach: ChatReach<R> = { inside: [], elsewhere: [], missing: [] };
  for (const repository of repositories) {
    const copy = localCopy(repository.repoKey, projects);
    if (!copy) { reach.missing.push(repository); continue; }
    const path = comparable(copy.rootPath);
    (path.startsWith(base + "/") ? reach.inside : reach.elsewhere).push({ repository, path: copy.rootPath });
  }
  return reach;
}

/** Esquece a pasta da organização neste computador. Os clones e os projetos
 * continuam onde estão: só a pasta lembrada sai. */
export function forgetOrganizationFolder(orgId: string) {
  try {
    localStorage.removeItem(FOLDER_KEY + orgId);
  } catch {
    // Sem armazenamento não havia pasta guardada.
  }
}
