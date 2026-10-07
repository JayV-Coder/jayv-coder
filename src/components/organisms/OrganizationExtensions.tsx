import { useEffect } from "react";
import { useT } from "@/modules/i18n";
import { loadOrgExtensions, orgExtensionsPath, useOrgExtensions } from "@/modules/orgExtensions";
import { can, type Organization } from "@/modules/organizations";
import { EmptyText } from "@/components/atoms";
import { SettingsSection } from "@/components/molecules";
import { Badge } from "@/components/ui/badge";
import { SiteDashboardButton } from "./SiteDashboardButton";

/** As abas MCP e Skills da organização aberta: o que ela dá a quem está nela,
 * como a sincronização trouxe, só para ler. Quem cadastra é o owner ou o
 * maintainer, no painel do site, e o botão leva à aba de lá. */
export function OrganizationExtensions({ organization, kind }: { organization: Organization; kind: "mcp" | "skills" }) {
  const t = useT();
  const data = useOrgExtensions((state) => state.data);
  useEffect(() => { void loadOrgExtensions(); }, []);
  const servers = (data?.mcp ?? []).filter((server) => server.org === organization.slug);
  const skills = (data?.skills ?? []).filter((skill) => skill.org === organization.slug);
  const count = kind === "mcp" ? servers.length : skills.length;

  return (
    <SettingsSection title={t(kind === "mcp" ? "settings.tab.mcp" : "settings.tab.skills")} description={t(kind === "mcp" ? "org.extensions.mcp.description" : "org.extensions.skills.description")}>
      {can.manage(organization.role) && <SiteDashboardButton path={orgExtensionsPath(organization.id, kind)} size="sm" />}
      {count === 0 ? <EmptyText>{t(kind === "mcp" ? "org.extensions.mcp.empty" : "org.extensions.skills.empty")}</EmptyText> : (
        <ul className="grid gap-2">
          {kind === "mcp" && servers.map((server) => (
            <li key={server.name} className="flex flex-wrap items-center gap-3 rounded-md border border-border px-3 py-2">
              <div className="grid min-w-0 flex-1 gap-0.5">
                <span className="flex items-center gap-2 text-sm font-medium">{server.name}<Badge variant="outline">{t(`mcp.transport.${server.transport}`)}</Badge></span>
                <code className="truncate font-mono text-xs text-muted-foreground">{server.transport === "stdio" ? server.command : server.url}</code>
              </div>
              {!server.enabled && <Badge variant="outline">{t("orgExtensions.off")}</Badge>}
            </li>
          ))}
          {kind === "skills" && skills.map((skill) => (
            <li key={skill.name} className="grid gap-0.5 rounded-md border border-border px-3 py-2">
              <span className="text-sm font-medium">{skill.name}</span>
              <span className="line-clamp-2 text-xs text-muted-foreground">{skill.description}</span>
            </li>
          ))}
        </ul>
      )}
    </SettingsSection>
  );
}
