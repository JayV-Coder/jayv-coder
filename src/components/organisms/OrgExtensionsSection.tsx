import { useEffect } from "react";
import { useT } from "@/modules/i18n";
import { loadOrgExtensions, useOrgExtensions } from "@/modules/orgExtensions";
import { SettingsSection } from "@/components/molecules";
import { Badge } from "@/components/ui/badge";

/** O que as organizações dão, em Configurações › MCP e › Skills: só leitura.
 * Entra por cima do que a pessoa tem nos projetos da organização, e quem
 * cadastra é o owner ou o maintainer, no painel do site. Sem nada, a seção
 * não aparece. */
export function OrgExtensionsSection({ kind }: { kind: "mcp" | "skills" }) {
  const t = useT();
  const data = useOrgExtensions((state) => state.data);
  useEffect(() => { void loadOrgExtensions(); }, []);
  const items = kind === "mcp" ? data?.mcp ?? [] : data?.skills ?? [];
  if (items.length === 0) return null;

  return (
    <SettingsSection title={t(kind === "mcp" ? "mcp.org.title" : "skills.org.title")} description={t(kind === "mcp" ? "mcp.org.description" : "skills.org.description")}>
      <ul className="grid gap-2">
        {data && kind === "mcp" && data.mcp.map((server) => (
          <li key={`${server.org}/${server.name}`} className="flex flex-wrap items-center gap-3 rounded-md border border-border px-3 py-2">
            <div className="grid min-w-0 flex-1 gap-0.5">
              <span className="flex items-center gap-2 text-sm font-medium">{server.name}<Badge variant="outline">{t(`mcp.transport.${server.transport}`)}</Badge></span>
              <code className="truncate font-mono text-xs text-muted-foreground">{server.transport === "stdio" ? server.command : server.url}</code>
            </div>
            {!server.enabled && <Badge variant="outline">{t("orgExtensions.off")}</Badge>}
            <Badge variant="secondary">{t("orgExtensions.from", { org: server.org })}</Badge>
          </li>
        ))}
        {data && kind === "skills" && data.skills.map((skill) => (
          <li key={`${skill.org}/${skill.name}`} className="flex flex-wrap items-center gap-3 rounded-md border border-border px-3 py-2">
            <div className="grid min-w-0 flex-1 gap-0.5">
              <span className="text-sm font-medium">{skill.name}</span>
              <span className="line-clamp-2 text-xs text-muted-foreground">{skill.description}</span>
            </div>
            <Badge variant="secondary">{t("orgExtensions.from", { org: skill.org })}</Badge>
          </li>
        ))}
      </ul>
    </SettingsSection>
  );
}
