import { AGENT_MECHANISMS, type AgentId, type Mechanism } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { useOrganizations } from "@/modules/organizations";
import { FormField, ToggleRow } from "@/components/molecules";

/** O que o agente faz além de ler e editar o projeto. Sem terminal, ninguém
 * responde ao pedido de aprovação dele, e o que não vem liberado aqui é
 * negado — a busca na web inclusive. Só aparecem os mecanismos que a CLI do
 * agente sabe ligar; o que uma organização bloqueia continua ligável fora dos
 * projetos dela, e a tela diz quem bloqueia. */
export function MechanismsField({ agent, selected, onChange }: { agent: AgentId; selected: Mechanism[]; onChange: (mechanisms: Mechanism[]) => void }) {
  const t = useT();
  const blocked = useOrganizations((state) => state.blockedMechanisms);
  const supported = AGENT_MECHANISMS[agent];
  if (supported.length === 0) return null;
  const toggle = (mechanism: Mechanism, on: boolean) =>
    onChange(on ? supported.filter((item) => item === mechanism || selected.includes(item)) : selected.filter((item) => item !== mechanism));

  return (
    <FormField label={t("agent.mechanisms")} hint={t("agent.mechanisms.hint")} wide>
      <div className="grid gap-2 sm:grid-cols-2">
        {supported.map((mechanism) => {
          const orgs = blocked[`${agent}/${mechanism}`] ?? [];
          const hint = t(`mechanism.${mechanism}.hint.${agent}` as Key);
          return (
            <ToggleRow
              key={mechanism}
              id={`${agent}-mechanism-${mechanism}`}
              label={t(`mechanism.${mechanism}` as Key)}
              hint={orgs.length > 0 ? `${hint} ${t("agent.mechanisms.blocked", { orgs: orgs.join(", ") })}` : hint}
              checked={selected.includes(mechanism)}
              onChange={(on) => toggle(mechanism, on)}
            />
          );
        })}
      </div>
    </FormField>
  );
}
