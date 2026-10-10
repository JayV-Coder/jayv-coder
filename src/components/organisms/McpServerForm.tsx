import { useMemo, useState } from "react";
import { isCustomMod, type CustomModOptions, type McpServer } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { lines, pairs, pairText } from "@/modules/mcp";
import { agentIds, agentLabel, useSettings } from "@/modules/settings";
import { FormField, SegmentedControl, ToggleRow } from "@/components/molecules";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

/** Os mods que recebem os servidores, cada um do jeito que sabe (e só com o
 * "Aprovar servidores MCP" dele ligado): todos os do app e os criados de API
 * com o protocolo da OpenAI, que o JayV chama como cliente MCP. O mod criado
 * de linha de comando não tem por onde recebê-los. */
export function useMcpAgents() {
  const agents = useSettings((state) => state.agents);
  return useMemo(() => agentIds(agents).filter((id) => {
    if (!isCustomMod(id)) return true;
    const options = agents.find((agent) => agent.id === id)?.options as CustomModOptions | undefined;
    return options?.kind === "api" && options.protocol === "openai";
  }).map((id) => ({ id: id as string, name: agentLabel(id, agents) })), [agents]);
}

/** Os campos de um servidor MCP. Argumentos, variáveis e cabeçalhos vão um
 * por linha, como num arquivo: é o jeito mais curto de colar o que a
 * documentação do servidor mostra. */
export function McpServerForm({ server, onChange, idPrefix }: { server: McpServer; onChange: (server: McpServer) => void; idPrefix: string }) {
  const t = useT();
  const mcpAgents = useMcpAgents();
  // O texto das listas fica como foi digitado; o servidor recebe o lido.
  const [args, setArgs] = useState(server.args.join("\n"));
  const [env, setEnv] = useState(pairText(server.env, "="));
  const [headers, setHeaders] = useState(pairText(server.headers, ":"));
  const id = (field: string) => `${idPrefix}-${field}`;
  const stdio = server.transport === "stdio";
  const toggleAgent = (agent: string, on: boolean) => {
    const all = server.agents.length === 0 ? mcpAgents.map((item) => item.id) : server.agents;
    const next = on ? [...new Set([...all, agent])] : all.filter((item) => item !== agent);
    // Todos marcados é o mesmo que nenhum escolhido: vale para os novos agentes.
    onChange({ ...server, agents: next.length === mcpAgents.length ? [] : next });
  };
  const receives = (agent: string) => server.agents.length === 0 || server.agents.includes(agent);

  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <FormField label={t("mcp.field.name")} htmlFor={id("name")} hint={t("mcp.field.name.hint")}>
        <Input id={id("name")} value={server.name} placeholder="github" onChange={(event) => onChange({ ...server, name: event.target.value })} />
      </FormField>
      <FormField label={t("mcp.field.transport")}>
        <SegmentedControl<McpServer["transport"]>
          label={t("mcp.field.transport")}
          value={server.transport}
          options={[{ value: "stdio", label: t("mcp.transport.stdio"), hint: t("mcp.transport.stdio.hint") }, { value: "http", label: t("mcp.transport.http"), hint: t("mcp.transport.http.hint") }]}
          onChange={(transport) => onChange({ ...server, transport })}
        />
      </FormField>
      {stdio ? (
        <>
          <FormField label={t("mcp.field.command")} htmlFor={id("command")} wide>
            <Input id={id("command")} value={server.command} placeholder="npx" className="font-mono" onChange={(event) => onChange({ ...server, command: event.target.value })} />
          </FormField>
          <FormField label={t("mcp.field.args")} htmlFor={id("args")} hint={t("mcp.field.args.hint")}>
            <Textarea id={id("args")} rows={3} value={args} placeholder={"-y\n@modelcontextprotocol/server-github"} className="font-mono text-xs" onChange={(event) => { setArgs(event.target.value); onChange({ ...server, args: lines(event.target.value) }); }} />
          </FormField>
          <FormField label={t("mcp.field.env")} htmlFor={id("env")} hint={t("mcp.field.env.hint")}>
            <Textarea id={id("env")} rows={3} value={env} placeholder="GITHUB_PERSONAL_ACCESS_TOKEN=…" className="font-mono text-xs" onChange={(event) => { setEnv(event.target.value); onChange({ ...server, env: pairs(event.target.value, "=") }); }} />
          </FormField>
        </>
      ) : (
        <>
          <FormField label={t("mcp.field.url")} htmlFor={id("url")} wide>
            <Input id={id("url")} value={server.url} placeholder="https://…/mcp" className="font-mono" onChange={(event) => onChange({ ...server, url: event.target.value })} />
          </FormField>
          <FormField label={t("mcp.field.headers")} htmlFor={id("headers")} hint={t("mcp.field.headers.hint")} wide>
            <Textarea id={id("headers")} rows={2} value={headers} placeholder="Authorization: Bearer …" className="font-mono text-xs" onChange={(event) => { setHeaders(event.target.value); onChange({ ...server, headers: pairs(event.target.value, ":") }); }} />
          </FormField>
        </>
      )}
      <FormField label={t("mcp.field.agents")} hint={t("mcp.field.agents.hint")} wide>
        <div className="flex flex-wrap gap-3">
          {mcpAgents.map((agent) => (
            <Label key={agent.id} htmlFor={id(`agent-${agent.id}`)} className="flex items-center gap-2 text-sm font-normal">
              <Checkbox id={id(`agent-${agent.id}`)} checked={receives(agent.id)} onCheckedChange={(checked) => toggleAgent(agent.id, checked === true)} />
              {agent.name}
            </Label>
          ))}
        </div>
      </FormField>
      <ToggleRow id={id("enabled")} label={t("mcp.field.enabled")} hint={t("mcp.field.enabled.hint")} checked={server.enabled} onChange={(enabled) => onChange({ ...server, enabled })} className="col-span-full" />
    </div>
  );
}
