import type { AgentSettings, ClaudeOptions, Mechanism } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { updateOptions, type ModelDraft } from "@/modules/settings";
import { toggleState, useEntitlements } from "@/modules/plans";
import { CheckList, FormField, OptionSelect, ToggleRow } from "@/components/molecules";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { MechanismsField } from "./MechanismsField";

type Mode = Exclude<ClaudeOptions["permissionMode"], "default">;
const PERMISSIONS: Mode[] = ["manual", "plan", "acceptEdits", "auto", "dontAsk", "bypassPermissions"];
const EFFORTS: ClaudeOptions["effort"][] = ["auto", "low", "medium", "high", "xhigh", "max"];
const TOOLS = ["Bash", "Edit", "Write", "NotebookEdit", "WebFetch", "WebSearch"];
const NONE = "__none__";
/** A ferramenta atrás de cada mecanismo: bloquear uma desliga o outro. */
const TOOL_OF: Partial<Record<Mechanism, string>> = { webSearch: "WebSearch", webFetch: "WebFetch", shell: "Bash" };

/** O que o Claude Code pode fazer e como pensa. */
export function ClaudeOptionsForm({ agent, models, problems }: { agent: AgentSettings<"claude">; models: ModelDraft[]; problems: Record<string, Key> }) {
  const t = useT();
  const options = agent.options;
  const set = (patch: Partial<ClaudeOptions>) => updateOptions("claude", patch);
  const reserves = [...new Set(models.map((model) => model.model.trim()).filter(Boolean))];
  const entitlements = useEntitlements();
  // Guardar as sessões é núcleo; o índice de símbolos segue o plano.
  const persist = toggleState(entitlements, "agentSessions", options.persistSessions);
  const symbols = toggleState(entitlements, "symbolIndex", options.symbolTools ?? false);
  // O modo seguro não sobe servidor MCP: o índice de símbolos não teria como chegar.
  const symbolsOff = options.safeMode && !symbols.disabled;
  // `default` é o nome antigo do `manual`.
  const mode: Mode = options.permissionMode === "default" ? "manual" : options.permissionMode;
  const hint = (reason: "required" | "outside" | null, text: string) => (reason ? `${t(reason === "required" ? "plans.required" : "plans.jevLocked")} · ${text}` : text);

  return (
    <div className="grid gap-5 sm:grid-cols-2">
      <FormField label={t("claude.permission")} htmlFor="claude-permission" hint={t(`claude.permission.${mode}.hint`)} wide>
        <OptionSelect id="claude-permission" value={mode} onChange={(permissionMode) => set({ permissionMode })}
          options={PERMISSIONS.map((value) => ({ value, label: t(`claude.permission.${value}`) }))} />
      </FormField>
      <FormField label={t("agent.effort")} htmlFor="claude-effort" hint={t("agent.effort.hint.auto")}>
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
      <MechanismsField agent="claude" selected={options.mechanisms ?? []}
        onChange={(mechanisms) => set({ mechanisms, blockedTools: options.blockedTools.filter((tool) => !mechanisms.some((mechanism) => TOOL_OF[mechanism] === tool)) })} />
      <FormField label={t("agent.tools")} hint={t("agent.tools.hint")} wide>
        <CheckList id="claude-tools" tone="danger" selected={options.blockedTools}
          onChange={(blockedTools) => set({ blockedTools, mechanisms: (options.mechanisms ?? []).filter((mechanism) => !blockedTools.includes(TOOL_OF[mechanism] ?? "")) })}
          items={TOOLS.map((value) => ({ value, label: t(`tool.${value}` as Key) }))} />
      </FormField>
      <FormField label={t("claude.prompt")} htmlFor="claude-prompt" hint={t("claude.prompt.hint", { count: options.appendSystemPrompt.length })} wide>
        <Textarea id="claude-prompt" rows={3} maxLength={4000} value={options.appendSystemPrompt} placeholder={t("claude.prompt.placeholder")}
          onChange={(event) => set({ appendSystemPrompt: event.target.value })} />
      </FormField>
      <ToggleRow id="claude-persist" label={t("claude.persist")} hint={hint(persist.reason, t("claude.persist.hint"))} checked={persist.checked} disabled={persist.disabled} onChange={(persistSessions) => set({ persistSessions })} />
      <ToggleRow id="claude-safe" label={t("claude.safe")} hint={t("claude.safe.hint")} checked={options.safeMode} onChange={(safeMode) => set(safeMode ? { safeMode, symbolTools: false } : { safeMode })} />
      <ToggleRow id="claude-symbols" label={t("claude.symbols")} hint={symbolsOff ? `${t("claude.symbols.safeMode")} · ${t("claude.symbols.hint")}` : hint(symbols.reason, t("claude.symbols.hint"))}
        checked={symbolsOff ? false : symbols.checked} disabled={symbols.disabled || symbolsOff} onChange={(symbolTools) => set({ symbolTools })} />
    </div>
  );
}
