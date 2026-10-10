import { create } from "zustand";
import { commands, type McpServer } from "@/modules/core";
import { reportError } from "@/modules/feedback";

/** Os servidores MCP gravados, a lista com as mudanças que ainda esperam o
 * Salvar das configurações e o rascunho aberto para conferir (o que veio do
 * `/mcp` do chat, da colagem na tela ou do botão de editar). Ligar, desligar,
 * editar, adicionar ou remover um servidor muda só `pending`: o banco só muda
 * no Salvar, e Descartar volta ao gravado. */
interface McpState {
  servers: McpServer[] | null;
  /** A lista como ficará ao salvar; `null` sem mudança pendente. */
  pending: McpServer[] | null;
  /** Os servidores em conferência, se foi um modelo que os montou e o nome
   * do servidor que a edição substitui (renomear não deixa a cópia antiga). */
  draft: { servers: McpServer[]; fromModel: boolean; editing: string | null } | null;
  drafting: boolean;
  saving: boolean;
}

export const useMcp = create<McpState>(() => ({ servers: null, pending: null, draft: null, drafting: false, saving: false }));

export const blankServer = (): McpServer => ({ name: "", transport: "stdio", command: "", args: [], env: {}, url: "", headers: {}, enabled: true, agents: [] });

/** Na ordem do banco (por nome): a mesma lista em outra ordem não é mudança. */
const byName = (servers: McpServer[]) => [...servers].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
const same = (a: McpServer[], b: McpServer[]) => JSON.stringify(byName(a)) === JSON.stringify(byName(b));

/** A lista que a tela mostra: a pendente ou, sem ela, a gravada. */
export const mcpList = (state: Pick<McpState, "servers" | "pending">) => state.pending ?? state.servers;

export const isMcpDirty = (state: Pick<McpState, "servers" | "pending">) => state.pending !== null && !same(state.pending, state.servers ?? []);

/** Relê o que está gravado. A mudança pendente continua de pé. */
export async function loadMcp() {
  try {
    useMcp.setState({ servers: await commands.getMcpServers() });
  } catch (error) {
    reportError(error);
    useMcp.setState((state) => ({ servers: state.servers ?? [] }));
  }
}

/** Troca a lista pendente. Voltar ao que está gravado tira a pendência. */
export function stageMcp(servers: McpServer[]) {
  useMcp.setState((state) => ({ pending: same(servers, state.servers ?? []) ? null : byName(servers) }));
}

/** Liga ou desliga um servidor (fica pendente até o Salvar). */
export function setMcpEnabled(name: string, enabled: boolean) {
  const list = mcpList(useMcp.getState()) ?? [];
  stageMcp(list.map((server) => (server.name === name ? { ...server, enabled } : server)));
}

/** Tira um servidor da lista (fica pendente até o Salvar). */
export function removeMcp(name: string) {
  stageMcp((mcpList(useMcp.getState()) ?? []).filter((server) => server.name !== name));
}

/** Grava a lista pendente: o Salvar das configurações chama. Devolve se
 * gravou (sem nada pendente, não há o que gravar). */
export async function saveMcpChanges(): Promise<boolean> {
  const { pending } = useMcp.getState();
  if (pending === null || !isMcpDirty(useMcp.getState())) {
    useMcp.setState({ pending: null });
    return true;
  }
  useMcp.setState({ saving: true });
  try {
    const servers = await commands.saveMcpServers(pending);
    useMcp.setState({ servers: byName(servers), pending: null });
    return true;
  } catch (error) {
    reportError(error);
    return false;
  } finally {
    useMcp.setState({ saving: false });
  }
}

/** Joga fora a lista pendente e relê a gravada. */
export function discardMcp() {
  useMcp.setState({ pending: null });
  void loadMcp();
}

/** Lê o texto colado (ou descrito) e abre o rascunho para conferir. */
export async function draftMcp(text: string): Promise<boolean> {
  if (!text.trim()) return false;
  useMcp.setState({ drafting: true });
  try {
    const draft = await commands.draftMcp(text);
    useMcp.setState({ draft: { ...draft, editing: null } });
    return true;
  } catch (error) {
    reportError(error);
    return false;
  } finally {
    useMcp.setState({ drafting: false });
  }
}

/** Abre o rascunho: `editing` é o nome do servidor que a edição substitui. */
export function openDraft(servers: McpServer[], fromModel = false, editing: string | null = null) {
  useMcp.setState({ draft: { servers, fromModel, editing } });
}

export function closeDraft() {
  useMcp.setState({ draft: null });
}

/** Confirma o rascunho na lista pendente: o servidor com o mesmo nome (ou o
 * que está sendo editado) é trocado, o novo entra. O núcleo confere a lista
 * inteira como o Salvar gravaria, sem gravar; recusada, o diálogo continua
 * aberto com o motivo. Nada vai ao banco antes do Salvar das configurações. */
export async function confirmDraft(servers: McpServer[]): Promise<boolean> {
  const state = useMcp.getState();
  const current = mcpList(state) ?? (await commands.getMcpServers().catch(() => []));
  if (state.servers === null) useMcp.setState({ servers: current });
  const replaced = new Set([...servers.map((server) => server.name.trim()), ...(state.draft?.editing ? [state.draft.editing] : [])]);
  try {
    const checked = await commands.checkMcpServers([...current.filter((server) => !replaced.has(server.name)), ...servers]);
    stageMcp(checked);
    closeDraft();
    return true;
  } catch (error) {
    reportError(error);
    return false;
  }
}

export { mcpCommand, lines, pairs, pairText } from "./text";
