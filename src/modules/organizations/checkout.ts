import type { LocalClone, Project } from "@/modules/core";

/** Um caminho comparável: sem espaço nas pontas e sem barra no fim. */
const pathKey = (path: string) => path.trim().replace(/[\\/]+$/, "");

/** O projeto deste computador que é um clone do repositório. Projeto sem pasta
 * (o que veio de outro computador pela sincronização) não conta. */
export function localCopy<P extends Pick<Project, "rootPath" | "repoKeys">>(repoKey: string, projects: P[]): P | null {
  return projects.find((project) => project.rootPath.trim() && (project.repoKeys ?? []).includes(repoKey)) ?? null;
}

/** Um clone da pasta que é de um repositório da organização: `project`, a
 * pasta já é um projeto; `elsewhere`, o repositório já é projeto em outra
 * pasta deste computador (trazer de novo faria dois); `new`, dá para trazer. */
export type FoundState = "project" | "elsewhere" | "new";

/** A pasta da organização comparada com o que o owner configurou no site. */
export interface FolderComparison<R> {
  /** Os repositórios da organização que têm clone na pasta. */
  found: { repository: R; path: string; state: FoundState; elsewhere: string | null }[];
  /** Os que não têm: para clonar. `local` é a pasta de um projeto deste
   * computador com o repositório, fora da pasta da organização. */
  missing: { repository: R; local: string | null }[];
  /** Os clones da pasta que a organização não tem. */
  outside: LocalClone[];
}

const depth = (path: string) => comparable(path).split("/").length;

/** Junta a busca na pasta (em qualquer profundidade) com os repositórios da
 * organização e os projetos deste computador. Um repositório clonado duas
 * vezes vale pelo clone mais raso. */
export function compareFolder<R extends { repoKey: string }>(
  repositories: R[],
  clones: LocalClone[],
  projects: Pick<Project, "rootPath" | "repoKeys">[],
): FolderComparison<R> {
  const folders = new Set(projects.map((project) => comparable(project.rootPath)).filter(Boolean));
  const configured = new Set(repositories.map((repository) => repository.repoKey));
  const ordered = [...clones].sort((a, b) => depth(a.path) - depth(b.path) || a.path.localeCompare(b.path));
  const comparison: FolderComparison<R> = { found: [], missing: [], outside: [] };
  for (const repository of repositories) {
    const clone = ordered.find((item) => item.keys.includes(repository.repoKey));
    const copy = localCopy(repository.repoKey, projects);
    if (!clone) {
      comparison.missing.push({ repository, local: copy?.rootPath ?? null });
      continue;
    }
    const state: FoundState = folders.has(comparable(clone.path)) ? "project" : copy ? "elsewhere" : "new";
    comparison.found.push({ repository, path: clone.path, state, elsewhere: state === "elsewhere" ? copy!.rootPath : null });
  }
  comparison.outside = clones.filter((clone) => !clone.keys.some((key) => configured.has(key)));
  return comparison;
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
