import { create } from "zustand";
import { commands, type OrgExtensions } from "@/modules/core";

/** Os servidores MCP e as skills que as organizações de quem usa dão. Descem
 * pela sincronização (`my_org_extensions`) e valem nos projetos delas; aqui
 * só se leem. Quem cadastra é o owner ou o maintainer, no painel do site. */
interface OrgExtensionsState { data: OrgExtensions | null }

export const useOrgExtensions = create<OrgExtensionsState>(() => ({ data: null }));

/** Relê o que está no cache local. Falhar deixa a lista vazia: a seção some. */
export async function loadOrgExtensions() {
  try {
    useOrgExtensions.setState({ data: await commands.getOrgExtensions() });
  } catch (error) {
    console.error(error);
    useOrgExtensions.setState({ data: { mcp: [], skills: [] } });
  }
}

/** O caminho do painel do site onde a organização cadastra o que dá. */
export const orgExtensionsPath = (orgId: string, tab: "mcp" | "skills") => `/organizations/${orgId}?tab=${tab}`;
