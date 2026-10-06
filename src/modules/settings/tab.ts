import { create } from "zustand";
import type { AgentId } from "@/modules/core";
import { navigate } from "@/modules/navigation";

/** As abas da tela de configurações: o app, o Jev, os servidores MCP e um
 * agente por aba. */
export type SettingsTab = "app" | "jev" | "mcp" | AgentId;

export const useSettingsTab = create<{ tab: SettingsTab }>(() => ({ tab: "app" }));

export function setSettingsTab(tab: SettingsTab) {
  useSettingsTab.setState({ tab });
}

/** Abre as configurações já numa aba (a paleta de comandos usa). */
export function openSettingsTab(tab: SettingsTab) {
  setSettingsTab(tab);
  navigate("settings");
}
