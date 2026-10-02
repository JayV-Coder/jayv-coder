import { useState } from "react";
import type { AgentId, Permission } from "@/modules/core";
import { notify, reportError } from "@/modules/feedback";
import { useT, type Key } from "@/modules/i18n";
import {
  can, clearPolicy, emptyPolicy, POLICY_AGENTS, POLICY_RULES, policyLines, policyProblems, savePolicy,
  type LlmPolicy, type OrganizationDetail, type Role,
} from "@/modules/organizations";
import { AGENT_LABELS } from "@/modules/settings";
import { ConfirmAction, FormField, OptionSelect, SettingsSection, ToggleRow } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Textarea } from "@/components/ui/textarea";

const PERMISSIONS: Permission[] = ["allow", "ask", "deny"];
/** O escopo da organização inteira no seletor; os outros são ids de repositório. */
const WHOLE = "org";

/** A política de LLM da organização e a de cada repositório. Ela só aperta: o
 * app a aplica por cima das configurações de cada pessoa. Quem é member vê o
 * formulário desativado. */
export function OrganizationPolicy({ detail, role }: { detail: OrganizationDetail; role: Role }) {
  const t = useT();
  const [scope, setScope] = useState(WHOLE);
  const repository = scope === WHOLE ? null : scope;
  const stored = detail.policies.find((policy) => policy.repositoryId === repository) ?? null;
  const has = (id: string | null) => detail.policies.some((policy) => policy.repositoryId === id);
  const options = [
    { value: WHOLE, label: t("policy.scope.org"), hint: t(has(null) ? "policy.scope.set" : "policy.scope.unset") },
    ...detail.repositories.map((item) => ({ value: item.id, label: item.repoKey, hint: t(has(item.id) ? "policy.scope.set" : "policy.scope.unset") })),
  ];

  return (
    <div className="grid gap-5">
      <SettingsSection title={t("policy.title")} description={t("policy.description")}>
        <FormField label={t("policy.scope")} htmlFor="policy-scope" hint={repository ? t("policy.scope.repoNote") : undefined}>
          <OptionSelect id="policy-scope" value={scope} options={options} onChange={setScope} />
        </FormField>
        {!can.manage(role) && <p className="text-xs text-muted-foreground">{t("policy.readOnly")}</p>}
      </SettingsSection>
      {/* A chave troca o formulário quando o escopo muda ou a política gravada volta do banco. */}
      <PolicyForm key={`${scope}:${stored?.updatedAt ?? "none"}`} orgId={detail.id} repository={repository} initial={stored} manages={can.manage(role)} />
    </div>
  );
}

function PolicyForm({ orgId, repository, initial, manages }: { orgId: string; repository: string | null; initial: LlmPolicy | null; manages: boolean }) {
  const t = useT();
  const [policy, setPolicy] = useState<LlmPolicy>(() => ({ ...emptyPolicy(), ...initial }));
  // Os campos de texto guardam o que foi digitado; a lista sai deles.
  const [models, setModels] = useState(policy.blocked_models.join("\n"));
  const [deny, setDeny] = useState(policy.deny.join("\n"));
  const [localOnly, setLocalOnly] = useState(policy.local_only.join("\n"));
  const [busy, setBusy] = useState(false);
  const draft: LlmPolicy = { ...policy, blocked_models: policyLines(models), deny: policyLines(deny), local_only: policyLines(localOnly) };
  const problems = policyProblems(draft);
  const update = (changes: Partial<LlmPolicy>) => setPolicy((current) => ({ ...current, ...changes }));

  const run = (action: () => Promise<void>, done: string) => {
    setBusy(true);
    action().then(() => notify(done)).catch(reportError).finally(() => setBusy(false));
  };

  const toggleAgent = (agent: AgentId, on: boolean) => {
    const current = policy.agents ?? [];
    update({ agents: on ? [...current, agent] : current.filter((item) => item !== agent) });
  };

  return (
    <fieldset disabled={!manages || busy} className="grid gap-5">
      {!initial && <p className="text-sm text-muted-foreground">{t("policy.none")}</p>}

      <SettingsSection title={t("policy.agents.title")}>
        <div className="grid gap-3">
          <ToggleRow id="policy-all-agents" label={t("policy.agents.all")} hint={t("policy.agents.all.hint")} checked={policy.agents === null}
            onChange={(all) => update({ agents: all ? null : [...POLICY_AGENTS] })} disabled={!manages} />
          {policy.agents !== null && (
            <div className="grid gap-2 sm:grid-cols-2">
              {POLICY_AGENTS.map((agent) => (
                <label key={agent} htmlFor={`policy-agent-${agent}`} className="flex items-center gap-2.5 rounded-md border border-border/60 px-3 py-2 text-sm">
                  <Checkbox id={`policy-agent-${agent}`} checked={policy.agents!.includes(agent)} onCheckedChange={(on) => toggleAgent(agent, on === true)} />
                  {AGENT_LABELS[agent]}
                </label>
              ))}
            </div>
          )}
          {problems.includes("agents") && <p role="alert" className="text-xs text-destructive">{t("policy.agents.invalid")}</p>}
          <FormField label={t("policy.models")} htmlFor="policy-models" hint={t("policy.models.hint")} error={problems.includes("models") ? t("policy.models.invalid") : null}>
            <Textarea id="policy-models" rows={4} spellCheck={false} className="font-mono text-xs" placeholder="claude/opus" value={models}
              aria-invalid={problems.includes("models") || undefined} onChange={(event) => setModels(event.target.value)} />
          </FormField>
          <ToggleRow id="policy-safe" label={t("policy.safe")} hint={t("policy.safe.hint")} checked={policy.safe_agents}
            onChange={(safe_agents) => update({ safe_agents })} disabled={!manages} />
        </div>
      </SettingsSection>

      <SettingsSection title={t("policy.privacy.title")} description={t("policy.privacy.description")}>
        <div className="grid gap-3">
          <ToggleRow id="policy-redact" label={t("app.redact")} hint={t("policy.redact.hint")} checked={policy.redact_secrets}
            onChange={(redact_secrets) => update({ redact_secrets })} disabled={!manages} />
          <div className="grid gap-4 sm:grid-cols-2">
            <FormField label={t("app.deny")} htmlFor="policy-deny" hint={t("app.deny.hint")} error={problems.includes("patterns") ? t("policy.patterns.invalid") : null}>
              <Textarea id="policy-deny" rows={5} spellCheck={false} className="font-mono text-xs" value={deny} onChange={(event) => setDeny(event.target.value)} />
            </FormField>
            <FormField label={t("app.localOnly")} htmlFor="policy-local-only" hint={t("app.localOnly.hint")}>
              <Textarea id="policy-local-only" rows={5} spellCheck={false} className="font-mono text-xs" value={localOnly} onChange={(event) => setLocalOnly(event.target.value)} />
            </FormField>
          </div>
        </div>
      </SettingsSection>

      <SettingsSection title={t("policy.exit.title")} description={t("policy.exit.description")}>
        <div className="grid gap-4 sm:grid-cols-3">
          {POLICY_RULES.map((rule) => {
            const field = `min_${rule}` as const;
            return (
              <FormField key={rule} label={t(`jev.exit.${rule}` as Key)} htmlFor={`policy-exit-${rule}`}>
                <OptionSelect id={`policy-exit-${rule}`} value={policy[field]} disabled={!manages} onChange={(value) => update({ [field]: value })}
                  options={PERMISSIONS.map((permission) => ({ value: permission, label: t(`permission.${permission}` as Key) }))} />
              </FormField>
            );
          })}
        </div>
      </SettingsSection>

      {manages && (
        <div className="flex flex-wrap gap-2">
          <Button disabled={busy || problems.length > 0} onClick={() => run(() => savePolicy(orgId, repository, draft), t("policy.saved"))}>{t("policy.save")}</Button>
          {initial && (
            <ConfirmAction title={t("policy.clear.title")} description={t("policy.clear.description")} confirm={t("policy.clear")}
              onConfirm={() => run(() => clearPolicy(orgId, repository), t("policy.cleared"))}>
              <Button variant="outline" disabled={busy}>{t("policy.clear")}</Button>
            </ConfirmAction>
          )}
        </div>
      )}
    </fieldset>
  );
}
