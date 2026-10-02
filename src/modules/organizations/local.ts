import { commands, type FoundRepository } from "@/modules/core";
import { navigate } from "@/modules/navigation";
import { folderName, loadWorkspace } from "@/modules/workspace";

/** Clona o repositório dentro da pasta; o núcleo já cria o projeto. */
export async function cloneRepository(repoKey: string, folder: string) {
  const project = await commands.cloneRepository(repoKey, folder);
  await loadWorkspace();
  return project;
}

/** Os clones dos repositórios dentro da pasta. */
export function scanFolder(folder: string, repoKeys: string[]) {
  return commands.scanRepositories(folder, repoKeys);
}

/** Cria um projeto para cada clone, com o nome da pasta, sem abrir chat: a
 * pessoa está trazendo vários de uma vez. */
export async function importClones(clones: FoundRepository[]) {
  try {
    for (const clone of clones) await commands.createProject(folderName(clone.path), clone.path);
  } finally {
    await loadWorkspace();
  }
}

/** Liga uma pasta que a pessoa já tem ao repositório, depois de conferir que
 * algum remote dela é ele. */
export async function linkFolder(repoKey: string, path: string) {
  const keys = await commands.folderRepoKeys(path);
  if (!keys.includes(repoKey)) throw { key: "repos.link.mismatch", params: { repo: repoKey } };
  await commands.createProject(folderName(path), path);
  await loadWorkspace();
}

/** O seletor de pastas do sistema; `null` quando a pessoa desiste. */
export async function pickFolder(title: string, defaultPath?: string | null): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const chosen = await open({ directory: true, multiple: false, defaultPath: defaultPath ?? undefined, title });
  return typeof chosen === "string" ? chosen : null;
}

/** Um chat novo no projeto da organização, cuja pasta é a da organização
 * neste computador: o agente enxerga todos os clones que estão nela. */
export async function openOrganizationChat(orgId: string, name: string, folder: string) {
  const project = await commands.organizationProject(orgId, name, folder);
  const chat = await commands.createChat(project.id);
  await loadWorkspace(chat.id);
  navigate("chat");
}
