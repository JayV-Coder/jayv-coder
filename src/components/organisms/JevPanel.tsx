import { ArrowDownIcon, ArrowUpIcon } from "lucide-react";
import { COMPLEXITIES, type AgentId, type CoreSettings, type CoreSnapshot, type Permission } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { AGENTS, AGENT_LABELS, updateCore } from "@/modules/settings";
import { AgentIcon } from "@/components/atoms";
import { FormField, OptionSelect, SettingsSection, ToggleRow } from "@/components/molecules";
import { Button } from "@/components/ui/button";
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
          <ToggleRow id="jev-review" label={t("jev.review")} hint={t("jev.review.hint")} checked={core.reviewChanges} onChange={(reviewChanges) => updateCore({ reviewChanges })} />
          <ToggleRow id="jev-plan-first" label={t("jev.planFirst")} hint={t("jev.planFirst.hint")} checked={core.planFirst} onChange={(planFirst) => updateCore({ planFirst })} />
          <ToggleRow id="jev-parallel" label={t("jev.parallel")} hint={t("jev.parallel.hint")} checked={core.parallelTasks} onChange={(parallelTasks) => updateCore({ parallelTasks })} />
          <AgentOrderField order={core.agentOrder} />
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

/** Quem ganha quando dois agentes empatam na nota: espalhados entre os chats,
 * ou na ordem escolhida aqui. A ordem guarda os quatro agentes. */
function AgentOrderField({ order }: { order: AgentId[] }) {
  const t = useT();
  const ordered = order.length > 0;
  const full = [...order, ...AGENTS.filter((agent) => !order.includes(agent))];
  const move = (index: number, step: -1 | 1) => {
    const next = [...full];
    [next[index], next[index + step]] = [next[index + step], next[index]];
    updateCore({ agentOrder: next });
  };
  return (
    <FormField label={t("jev.agentOrder")} htmlFor="jev-agent-order" hint={t("jev.agentOrder.hint")}>
      <div className="grid gap-2">
        <OptionSelect id="jev-agent-order" value={ordered ? "order" : "spread"} onChange={(mode) => updateCore({ agentOrder: mode === "order" ? full : [] })}
          options={[{ value: "spread", label: t("jev.agentOrder.spread") }, { value: "order", label: t("jev.agentOrder.order") }]} />
        {ordered && (
          <ol className="grid gap-1">
            {full.map((agent, index) => (
              <li key={agent} className="flex items-center gap-2 rounded-md border px-2 py-1 text-sm">
                <span className="w-4 text-muted-foreground">{index + 1}</span>
                <AgentIcon agent={agent} className="size-4" />
                <span className="flex-1">{AGENT_LABELS[agent]}</span>
                <Button size="icon-sm" variant="ghost" disabled={index === 0} onClick={() => move(index, -1)} aria-label={t("jev.agentOrder.up", { agent: AGENT_LABELS[agent] })}><ArrowUpIcon /></Button>
                <Button size="icon-sm" variant="ghost" disabled={index === full.length - 1} onClick={() => move(index, 1)} aria-label={t("jev.agentOrder.down", { agent: AGENT_LABELS[agent] })}><ArrowDownIcon /></Button>
              </li>
            ))}
          </ol>
        )}
      </div>
    </FormField>
  );
}
