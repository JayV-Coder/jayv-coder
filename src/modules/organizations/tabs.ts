import type { Key } from "@/modules/i18n";
import type { OrganizationTab } from "./index";

/** O recurso do plano que liga cada aba (as outras são de todo membro). */
export type OrganizationTabFeature = "stats" | "gateBoard" | "mcp" | "skills";

export interface OrganizationTabItem {
  tab: OrganizationTab;
  label: Key;
  feature?: OrganizationTabFeature;
}

/** As abas da organização, na ordem da página, da paleta e do menu lateral do
 * ambiente da organização. */
export const ORGANIZATION_TABS: OrganizationTabItem[] = [
  { tab: "projects", label: "org.tab.projects" },
  { tab: "stats", label: "org.tab.stats", feature: "stats" },
  { tab: "gate", label: "org.tab.gate", feature: "gateBoard" },
  { tab: "members", label: "org.tab.members" },
  { tab: "repositories", label: "org.tab.repositories" },
  { tab: "mcp", label: "settings.tab.mcp", feature: "mcp" },
  { tab: "skills", label: "settings.tab.skills", feature: "skills" },
];
