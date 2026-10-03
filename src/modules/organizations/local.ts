import { commands, type FoundRepository } from "@/modules/core";
import { navigate } from "@/modules/navigation";
import { chatsOf, folderName, loadWorkspace, openChat, useWorkspace } from "@/modules/workspace";
import { organizationChatOf } from "./scope";

/** Clona o repositório dentro da pasta; o núcleo já cria o projeto. */
export async function cloneRepository(repoKey: string, folder: string) {
  const project = await commands.cloneRepository(repoKey, folder);
  await loadWorkspace();
  return project;
}

/** Os repositórios dentro da pasta do chat com o branch e o status de cada um. */
export function repositoryStates(folder: string) {
  return commands.repositoryStates(folder);
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

/** O chat no projeto da organização, cuja pasta é a da organização neste
 * computador: o agente enxerga todos os clones que estão nela. Quando o
 * projeto já tem chat, volta para o mais recente em vez de abrir outro. */
export async function openOrganizationChat(orgId: string, name: string, folder: string) {
  const project = await commands.organizationProject(orgId, name, folder);
  const chat = chatsOf(useWorkspace.getState().data, project.id)[0] ?? await commands.createChat(project.id);
  await loadWorkspace(chat.id);
  navigate("chat");
}

/** Vai direto ao chat da organização que já existe neste computador; `false`
 * quando ainda não há nenhum e é preciso escolher a pasta. */
export function resumeOrganizationChat(orgId: string): boolean {
  const chat = organizationChatOf(useWorkspace.getState().data, orgId);
  if (!chat) return false;
  openChat(chat.id);
  return true;
}
