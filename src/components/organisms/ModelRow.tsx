import type { Capability, CostClass, KnownModel, Speed } from "@/modules/core";
import { useLocale, useT, type Key } from "@/modules/i18n";
import { MODEL_PATTERN, pickModel, removeModel, updateModel, type ModelDraft } from "@/modules/settings";
import { CheckList, FormField, OptionSelect } from "@/components/molecules";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

const CUSTOM = "__custom__";
const CAPABILITIES: Capability[] = ["chat", "code", "reasoning", "tools"];
const COSTS: CostClass[] = ["free", "low", "medium", "high"];
const SPEEDS: Speed[] = ["fast", "medium", "slow"];
const WINDOWS = [32000, 64000, 128000, 200000, 272000, 400000, 1000000];

/** Um modelo do agente. O identificador vem do catálogo sempre que possível;
 * digitar é a exceção, e o campo avisa na hora quando o texto não serve. */
export function ModelRow({ model, catalog, taken, problem }: { model: ModelDraft; catalog: KnownModel[]; taken: Set<string>; problem?: Key }) {
  const t = useT();
  const locale = useLocale();
  const id = (field: string) => `${model.uid}-${field}`;
  const known = catalog.some((item) => item.id === model.model);
  const compact = new Intl.NumberFormat(locale, { notation: "compact" });
  const windows = [...new Set([...WINDOWS, model.contextWindow])].sort((a, b) => a - b);
  const typed = model.model.trim();
  const typing = !known && typed !== "" && !MODEL_PATTERN.test(typed);

  return (
    <div className={cn("grid gap-4 rounded-lg border px-4 py-4", problem ? "border-destructive/50" : "border-border/60", !model.enabled && "opacity-70")}>
      <div className="flex flex-wrap items-end gap-3">
        <Switch checked={model.enabled} onCheckedChange={(enabled) => updateModel(model.uid, { enabled })} aria-label={t("model.enabled")} title={t("model.enabled")} className="mb-2.5" />
        <FormField label={t("model.id")} htmlFor={id("model")} className="min-w-56 flex-1">
          <OptionSelect
            id={id("model")}
            value={known ? model.model : CUSTOM}
            onChange={(value) => (value === CUSTOM ? updateModel(model.uid, { model: "" }) : pickModel(model.uid, value))}
            options={[
              ...catalog.map((item) => ({ value: item.id, label: item.label, hint: item.id, disabled: item.id !== model.model && taken.has(item.id) })),
              { value: CUSTOM, label: t("model.custom") },
            ]}
          />
        </FormField>
        {!known && (
          <FormField label={t("model.custom.label")} htmlFor={id("custom")} className="min-w-56 flex-1">
            <Input
              id={id("custom")}
              value={model.model}
              autoFocus={model.model === ""}
              spellCheck={false}
              placeholder={t("model.custom.placeholder")}
              aria-invalid={typing || problem === "model.invalid" || undefined}
              onChange={(event) => updateModel(model.uid, { model: event.target.value.trim() })}
              className="font-mono"
            />
          </FormField>
        )}
        <button type="button" title={t("model.remove")} aria-label={t("model.remove")} onClick={() => removeModel(model.uid)} className="mb-1.5 px-2 text-xl leading-none text-muted-foreground hover:text-destructive">×</button>
      </div>

      <div className="grid gap-4 sm:grid-cols-3">
        <FormField label={t("model.context")} htmlFor={id("context")}>
          <OptionSelect id={id("context")} value={String(model.contextWindow)} onChange={(value) => updateModel(model.uid, { contextWindow: Number(value) })}
            options={windows.map((size) => ({ value: String(size), label: t("model.context.value", { size: compact.format(size) }) }))} />
        </FormField>
        <FormField label={t("model.cost")} htmlFor={id("cost")}>
          <OptionSelect id={id("cost")} value={model.costClass} onChange={(costClass) => updateModel(model.uid, { costClass })}
            options={COSTS.map((value) => ({ value, label: t(`cost.${value}`) }))} />
        </FormField>
        <FormField label={t("model.speed")} htmlFor={id("speed")}>
          <OptionSelect id={id("speed")} value={model.speed} onChange={(speed) => updateModel(model.uid, { speed })}
            options={SPEEDS.map((value) => ({ value, label: t(`speed.${value}`) }))} />
        </FormField>
      </div>

      <FormField label={t("model.capabilities")} hint={t("model.capabilities.hint")}>
        <CheckList id={id("caps")} selected={model.capabilities} onChange={(capabilities) => updateModel(model.uid, { capabilities: capabilities as Capability[] })}
          items={CAPABILITIES.map((value) => ({ value, label: t(`capability.${value}`) }))} />
      </FormField>

      {(problem || typing) && <p role="alert" className="text-[11.5px] text-destructive">{t(problem ?? "model.invalid")}</p>}
    </div>
  );
}
