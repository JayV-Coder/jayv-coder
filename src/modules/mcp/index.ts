import { create } from "zustand";
import { commands, type McpServer } from "@/modules/core";
import { reportError } from "@/modules/feedback";

/** Os servidores MCP gravados e o rascunho aberto para conferir (o que veio
 * do `/mcp` do chat ou da colagem na tela). Nada do rascunho é gravado antes
 * de o desenvolvedor salvar. */
interface McpState {
  servers: McpServer[] | null;
  /** Os servidores em conferência e se foi um modelo que os montou. */
  draft: { servers: McpServer[]; fromModel: boolean } | null;
  drafting: boolean;
}

export const useMcp = create<McpState>(() => ({ servers: null, draft: null, drafting: false }));

export const blankServer = (): McpServer => ({ name: "", transport: "stdio", command: "", args: [], env: {}, url: "", headers: {}, enabled: true, agents: [] });

export async function loadMcp() {
  try {
    useMcp.setState({ servers: await commands.getMcpServers() });
  } catch (error) {
    reportError(error);
    useMcp.setState({ servers: [] });
  }
}

/** Grava a lista inteira. Devolve se gravou: o formulário só fecha então. */
export async function saveMcp(servers: McpServer[]): Promise<boolean> {
  try {
    useMcp.setState({ servers: await commands.saveMcpServers(servers) });
    return true;
  } catch (error) {
    reportError(error);
    return false;
  }
}

/** Lê o texto colado (ou descrito) e abre o rascunho para conferir. */
export async function draftMcp(text: string): Promise<boolean> {
  if (!text.trim()) return false;
  useMcp.setState({ drafting: true });
  try {
    const draft = await commands.draftMcp(text);
    useMcp.setState({ draft });
    return true;
  } catch (error) {
    reportError(error);
    return false;
  } finally {
    useMcp.setState({ drafting: false });
  }
}

export function openDraft(servers: McpServer[], fromModel = false) {
  useMcp.setState({ draft: { servers, fromModel } });
}

export function closeDraft() {
  useMcp.setState({ draft: null });
}

/** Confirma o rascunho: o servidor com o mesmo nome é trocado, o novo entra. */
export async function confirmDraft(servers: McpServer[]): Promise<boolean> {
  const current = useMcp.getState().servers ?? (await commands.getMcpServers().catch(() => []));
  const names = new Set(servers.map((server) => server.name));
  const saved = await saveMcp([...current.filter((server) => !names.has(server.name)), ...servers]);
  if (saved) closeDraft();
  return saved;
}

export { mcpCommand, lines, pairs, pairText } from "./text";
