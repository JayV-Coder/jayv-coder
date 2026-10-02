import { COMPLEXITIES, type CoreSettings, type CoreSnapshot, type Permission } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { updateCore } from "@/modules/settings";
import { FormField, OptionSelect, SettingsSection, ToggleRow } from "@/components/molecules";
import { Input } from "@/components/ui/input";

const THRESHOLDS = ["0.5", "0.55", "0.6", "0.65", "0.7", "0.75", "0.8", "0.85", "0.9", "0.95"];
const CACHE_TTLS = [0, 300, 900, 1800, 3600, 7200, 21600, 86400];
const PERMISSIONS: Permission[] = ["allow", "ask", "deny"];
const EXIT_RULES = ["read", "write", "shell"] as const;
const SCOPE_KEYS: Key[] = ["scope.0", "scope.1", "scope.2"];
const percent = (value: number) => `${Math.round(value * 100)}%`;

/** O Jev por dentro: como ele roteia, quanto contexto cada pedido leva, quanto
 * tempo o contexto vale e o que a portaria de saída deixa passar. Os números da
 * portaria de entrada vêm do Supabase e aqui só se leem. */
export function JevPanel({ core, snapshot }: { core: CoreSettings; snapshot: CoreSnapshot }) {
  const t = useT();
  const [minBudget, maxBudget] = snapshot.budgetRange;
  const duration = (seconds: number) =>
    seconds === 0 ? t("jev.cache.off") : seconds % 3600 === 0 ? t("jev.cache.hours", { count: seconds / 3600 }) : t("jev.cache.minutes", { count: Math.round(seconds / 60) });
  const ttls = [...new Set([...CACHE_TTLS, core.cacheTtl])].sort((a, b) => a - b);

  return (
    <div className="grid gap-5">
      <SettingsSection title={t("jev.section.routing")} description={t("jev.section.routing.description")}>
        <div className="grid gap-3">
          <ToggleRow id="jev-adaptive" label={t("jev.adaptive")} hint={t("jev.adaptive.hint")} checked={core.adaptiveRouting} onChange={(adaptiveRouting) => updateCore({ adaptiveRouting })} />
          <ToggleRow id="jev-local" label={t("jev.preferLocal")} hint={t("jev.preferLocal.hint")} checked={core.preferLocal} onChange={(preferLocal) => updateCore({ preferLocal })} />
          <FormField label={t("jev.confidence")} htmlFor="jev-confidence" hint={t("jev.confidence.hint")}>
            <OptionSelect id="jev-confidence" value={String(core.confidenceThreshold)} onChange={(value) => updateCore({ confidenceThreshold: Number(value) })}
              options={[...new Set([...THRESHOLDS, String(core.confidenceThreshold)])].map((value) => ({ value, label: percent(Number(value)) }))} />
          </FormField>
        </div>
      </SettingsSection>

      <SettingsSection title={t("jev.section.budgets")} description={t("jev.section.budgets.description", { min: minBudget, max: maxBudget })}>
        <div className="grid gap-4 sm:grid-cols-2">
          {COMPLEXITIES.map((level) => {
            const value = core.budgets[level];
            const invalid = !(value >= minBudget && value <= maxBudget);
            return (
              <FormField key={level} label={t(`complexity.${level}` as Key)} htmlFor={`jev-budget-${level}`} error={invalid ? t("jev.budget.invalid", { min: minBudget, max: maxBudget }) : null}>
                <Input id={`jev-budget-${level}`} type="number" inputMode="numeric" min={minBudget} max={maxBudget} step={500} value={Number.isFinite(value) ? value : ""}
                  aria-invalid={invalid || undefined} className="font-mono"
                  onChange={(event) => updateCore({ budgets: { ...core.budgets, [level]: event.target.valueAsNumber } })} />
              </FormField>
            );
          })}
        </div>
      </SettingsSection>

      <SettingsSection title={t("jev.section.cache")} description={t("jev.section.cache.description")}>
        <FormField label={t("jev.cache")} htmlFor="jev-cache">
          <OptionSelect id="jev-cache" value={String(core.cacheTtl)} onChange={(value) => updateCore({ cacheTtl: Number(value) })}
            options={ttls.map((seconds) => ({ value: String(seconds), label: duration(seconds) }))} />
        </FormField>
      </SettingsSection>

      <SettingsSection title={t("jev.section.exit")} description={t("jev.section.exit.description")}>
        <div className="grid gap-4 sm:grid-cols-3">
          {EXIT_RULES.map((rule) => (
            <FormField key={rule} label={t(`jev.exit.${rule}` as Key)} htmlFor={`jev-exit-${rule}`} hint={t(`jev.exit.${rule}.hint` as Key)}>
              <OptionSelect id={`jev-exit-${rule}`} value={core.exitRules[rule]} onChange={(value) => updateCore({ exitRules: { ...core.exitRules, [rule]: value } })}
                options={PERMISSIONS.map((permission) => ({ value: permission, label: t(`permission.${permission}` as Key), hint: t(`permission.${permission}.hint` as Key) }))} />
            </FormField>
          ))}
        </div>
      </SettingsSection>

      <SettingsSection title={t("jev.section.gate")} description={t("jev.section.gate.description")}>
        <dl className="grid gap-x-6 gap-y-2.5 text-sm sm:grid-cols-2">
          {snapshot.gate.scopeDemand.map((demand, level) => (
            <div key={level} className="flex justify-between gap-3 border-b border-border/40 pb-1.5">
              <dt className="text-muted-foreground">{t("jev.gate.demand", { scope: t(SCOPE_KEYS[level]) })}</dt>
              <dd className="font-mono">{percent(demand)}</dd>
            </div>
          ))}
          <div className="flex justify-between gap-3 border-b border-border/40 pb-1.5">
            <dt className="text-muted-foreground">{t("jev.gate.margin")}</dt>
            <dd className="font-mono">{percent(snapshot.gate.blockMargin)}</dd>
          </div>
          {Object.entries(snapshot.gate.weights).map(([criterion, weight]) => (
            <div key={criterion} className="flex justify-between gap-3 border-b border-border/40 pb-1.5">
              <dt className="text-muted-foreground">{t("jev.gate.weight", { criterion: t(`criterion.${criterion}` as Key) })}</dt>
              <dd className="font-mono">{percent(weight)}</dd>
            </div>
          ))}
          <div className="flex justify-between gap-3 border-b border-border/40 pb-1.5">
            <dt className="text-muted-foreground">{t("jev.gate.noulLine")}</dt>
            <dd className="font-mono">{percent(snapshot.gate.noulLine)}</dd>
          </div>
        </dl>
      </SettingsSection>
    </div>
  );
}
