import { useMemo } from "react";
import { environmentOptions, switchEnvironment, useEnvironment } from "@/modules/environments";
import { useT } from "@/modules/i18n";
import { useOrganizations } from "@/modules/organizations";
import { allows, useEntitlements } from "@/modules/plans";
import { OptionSelect } from "@/components/molecules";

/** O ambiente aberto: o pessoal ou o de uma organização. Cada um tem os seus
 * projetos, chats, notas, uso e configurações; escolher outro recarrega a
 * janela com os dados dele. Sem organização nenhuma, só há o pessoal e o
 * seletor não aparece. */
export function EnvironmentSwitch() {
  const t = useT();
  const organizations = useOrganizations((state) => state.list);
  const rights = useEntitlements();
  const active = useEnvironment((state) => state.active);
  const switching = useEnvironment((state) => state.switching);
  const options = useMemo(() => environmentOptions(organizations).map((item) => ({
    value: item.id,
    label: item.kind === "personal" ? t("environment.personal") : item.name ?? item.id,
  })), [organizations, t]);
  if (!allows(rights, "organizations") || options.length < 2) return null;
  return (
    <div data-tour="environment-switch" className="grid gap-1 px-1.5 pb-4">
      <span className="font-mono text-caption font-medium tracking-wider text-sidebar-muted uppercase">{t("environment.label")}</span>
      <OptionSelect
        label={t("environment.label")}
        value={options.some((option) => option.value === active) ? active : options[0].value}
        options={options}
        disabled={switching}
        onChange={(next) => void switchEnvironment(next)}
      />
      {switching && <small className="text-caption text-sidebar-muted">{t("environment.switching")}</small>}
    </div>
  );
}
