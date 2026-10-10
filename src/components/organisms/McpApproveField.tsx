import { isApiAgent, type AgentSettings } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { useOrganizations } from "@/modules/organizations";
import { updateOptions } from "@/modules/settings";
import { ToggleRow } from "@/components/molecules";

/** "Aprovar servidores MCP": o mesmo interruptor para todo agente. Ligado, os
 * servidores da aba MCP chegam a ele em cada pedido e as ferramentas rodam sem
 * pergunta, porque ninguém as aprova enquanto o agente trabalha. A organização
 * pode bloquear (`agente/mcp`): fora dos projetos dela continua ligável, e a
 * tela diz quem bloqueia. */
export function McpApproveField({ agent }: { agent: AgentSettings }) {
  const t = useT();
  const blocked = useOrganizations((state) => state.blockedMechanisms[`${agent.id}/mcp`]) ?? [];
  const options = agent.options as { approveMcps?: boolean };
  const kind = isApiAgent(agent) ? "gateway" : agent.id === "cursor" ? "cursor" : "cli";
  const hint = t(`agent.approveMcps.hint.${kind}` as Key);
  return (
    <div className="mt-5 grid gap-2">
      <ToggleRow
        id={`${agent.id}-approve-mcps`}
        label={t("agent.approveMcps")}
        hint={blocked.length > 0 ? `${hint} ${t("agent.mechanisms.blocked", { orgs: blocked.join(", ") })}` : hint}
        checked={options.approveMcps === true}
        onChange={(approveMcps) => updateOptions(agent.id, { approveMcps } as never)}
      />
    </div>
  );
}
