import { useEffect, useState } from "react";
import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react";
import { useT } from "@/modules/i18n";
import { blankServer, draftMcp, loadMcp, mcpList, openDraft, removeMcp, setMcpEnabled, useMcp } from "@/modules/mcp";
import { LoadingNote } from "@/components/atoms";
import { SettingsSection } from "@/components/molecules";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { MCP_AGENTS } from "./McpServerForm";

/** Configurações › MCP: os servidores que o JayV entrega aos agentes em cada
 * pedido. Ligar, desligar, editar, adicionar e remover mudam só a lista da
 * tela, marcada como não salva: o banco muda no Salvar da página. Os
 * servidores das organizações não entram aqui: ficam no ambiente de cada
 * organização (MCP, no menu lateral dela). */
export function McpPanel() {
  const t = useT();
  const servers = useMcp(mcpList);
  const saved = useMcp((state) => state.servers);
  const drafting = useMcp((state) => state.drafting);
  const [pasted, setPasted] = useState("");
  // A tela de configurações relê ao abrir; isto só cobre a primeira vez.
  useEffect(() => { if (useMcp.getState().servers === null) void loadMcp(); }, []);
  if (servers === null) return <LoadingNote>{t("settings.loading")}</LoadingNote>;

  const agentsOf = (agents: string[]) => agents.length === 0 ? t("mcp.agents.all") : agents.map((id) => MCP_AGENTS.find((agent) => agent.id === id)?.name ?? id).join(", ");
  // Novo ou mudado desde o que está gravado: ganha o selo de não salvo.
  const unsaved = (name: string) => {
    const before = saved?.find((server) => server.name === name);
    const now = servers.find((server) => server.name === name);
    return !before || JSON.stringify(before) !== JSON.stringify(now);
  };

  return (
    <div className="grid gap-5">
      <SettingsSection
        title={t("mcp.title")}
        description={t("mcp.description")}
        action={<Button type="button" size="sm" variant="outline" onClick={() => openDraft([blankServer()])}><PlusIcon aria-hidden="true" />{t("mcp.add")}</Button>}
      >
        {servers.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("mcp.empty")}</p>
        ) : (
          <ul className="grid gap-2">
            {servers.map((server) => (
              <li key={server.name} className="flex flex-wrap items-center gap-3 rounded-md border border-border px-3 py-2">
                <Switch checked={server.enabled} aria-label={t("mcp.field.enabled")} onCheckedChange={(enabled) => setMcpEnabled(server.name, enabled)} />
                <div className="grid min-w-0 flex-1 gap-0.5">
                  <span className="flex flex-wrap items-center gap-2 text-sm font-medium">
                    {server.name}
                    <Badge variant="outline">{t(`mcp.transport.${server.transport}`)}</Badge>
                    {unsaved(server.name) && <Badge variant="warning">{t("settings.notSaved")}</Badge>}
                  </span>
                  <code className="truncate font-mono text-xs text-muted-foreground">{server.transport === "stdio" ? [server.command, ...server.args].join(" ") : server.url}</code>
                  <span className="text-xs text-muted-foreground">{agentsOf(server.agents)}</span>
                </div>
                <Button type="button" variant="ghost" size="icon-sm" aria-label={t("mcp.edit", { name: server.name })} title={t("mcp.edit", { name: server.name })} onClick={() => openDraft([server], false, server.name)}><PencilIcon aria-hidden="true" /></Button>
                <Button type="button" variant="ghost" size="icon-sm" aria-label={t("mcp.delete", { name: server.name })} title={t("mcp.delete", { name: server.name })} onClick={() => removeMcp(server.name)}><Trash2Icon aria-hidden="true" /></Button>
              </li>
            ))}
          </ul>
        )}
        <p className="text-xs leading-snug text-muted-foreground">{t("mcp.notes")}</p>
      </SettingsSection>
      <SettingsSection title={t("mcp.paste.title")} description={t("mcp.paste.description")}>
        <Textarea rows={5} value={pasted} placeholder={t("mcp.paste.placeholder")} className="font-mono text-xs" onChange={(event) => setPasted(event.target.value)} />
        <div className="flex justify-end">
          <Button type="button" disabled={drafting || !pasted.trim()} onClick={() => void draftMcp(pasted).then((ok) => { if (ok) setPasted(""); })}>
            {t(drafting ? "mcp.paste.reading" : "mcp.paste.read")}
          </Button>
        </div>
      </SettingsSection>
    </div>
  );
}
