import type { AgentSettings, ClaudeOptions } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { updateOptions, type ModelDraft } from "@/modules/settings";
import { CheckList, FormField, OptionSelect, ToggleRow } from "@/components/molecules";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";

const PERMISSIONS: ClaudeOptions["permissionMode"][] = ["default", "plan", "acceptEdits", "auto", "bypassPermissions"];
const EFFORTS: ClaudeOptions["effort"][] = ["default", "low", "medium", "high", "xhigh", "max"];
const TOOLS = ["Bash", "Edit", "Write", "NotebookEdit", "WebFetch", "WebSearch"];
const NONE = "__none__";

/** O que o Claude Code pode fazer e como pensa. */
export function ClaudeOptionsForm({ agent, models, problems }: { agent: AgentSettings<"claude">; models: ModelDraft[]; problems: Record<string, Key> }) {
  const t = useT();
  const options = agent.options;
  const set = (patch: Partial<ClaudeOptions>) => updateOptions("claude", patch);
  const reserves = [...new Set(models.map((model) => model.model.trim()).filter(Boolean))];

  return (
    <div className="grid gap-5 sm:grid-cols-2">
      <FormField label={t("claude.permission")} htmlFor="claude-permission" hint={t(`claude.permission.${options.permissionMode}.hint`)} wide>
        <OptionSelect id="claude-permission" value={options.permissionMode} onChange={(permissionMode) => set({ permissionMode })}
          options={PERMISSIONS.map((value) => ({ value, label: t(`claude.permission.${value}`) }))} />
      </FormField>
      <FormField label={t("agent.effort")} htmlFor="claude-effort" hint={t("agent.effort.hint")}>
        <OptionSelect id="claude-effort" value={options.effort} onChange={(effort) => set({ effort })}
          options={EFFORTS.map((value) => ({ value, label: t(`effort.${value}`) }))} />
      </FormField>
      <FormField label={t("claude.fallback")} htmlFor="claude-fallback" hint={t("claude.fallback.hint")}>
        <OptionSelect id="claude-fallback" value={options.fallbackModel || NONE} onChange={(value) => set({ fallbackModel: value === NONE ? "" : value })}
          options={[{ value: NONE, label: t("claude.fallback.none") }, ...reserves.map((value) => ({ value, label: value }))]} />
      </FormField>
      <FormField label={t("claude.budget")} htmlFor="claude-budget" hint={t("claude.budget.hint")} error={problems.budget && t(problems.budget)}>
        <Input
          id="claude-budget"
          type="number"
          inputMode="decimal"
          min={0.01}
          max={1000}
          step={0.5}
          placeholder={t("claude.budget.none")}
          value={options.maxBudgetUsd ?? ""}
          aria-invalid={problems.budget ? true : undefined}
          onChange={(event) => set({ maxBudgetUsd: event.target.value === "" ? null : Number(event.target.value) })}
        />
      </FormField>
      <FormField label={t("agent.tools")} hint={t("agent.tools.hint")} wide>
        <CheckList id="claude-tools" tone="danger" selected={options.blockedTools} onChange={(blockedTools) => set({ blockedTools })}
          items={TOOLS.map((value) => ({ value, label: t(`tool.${value}` as Key) }))} />
      </FormField>
      <FormField label={t("claude.prompt")} htmlFor="claude-prompt" hint={t("claude.prompt.hint", { count: options.appendSystemPrompt.length })} wide>
        <Textarea id="claude-prompt" rows={3} maxLength={4000} value={options.appendSystemPrompt} placeholder={t("claude.prompt.placeholder")}
          onChange={(event) => set({ appendSystemPrompt: event.target.value })} />
      </FormField>
      <ToggleRow id="claude-persist" label={t("claude.persist")} hint={t("claude.persist.hint")} checked={options.persistSessions} onChange={(persistSessions) => set({ persistSessions })} />
      <ToggleRow id="claude-safe" label={t("claude.safe")} hint={t("claude.safe.hint")} checked={options.safeMode} onChange={(safeMode) => set({ safeMode })} />
    </div>
  );
}
