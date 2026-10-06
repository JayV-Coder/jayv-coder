import type { AgentId, AgentSettings } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { checkGateway, isAgentsDirty, updateOptions, useSettings } from "@/modules/settings";
import { FormField } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** O endereço e a chave de um gateway de API (OpenRouter, LiteLLM). A chave
 * fica só neste computador: depois de salva, a tela só sabe que ela existe. */
export function GatewayConnection({ agent, problems }: { agent: AgentSettings<"openrouter">; problems: Record<string, Key> }) {
  const t = useT();
  const id: AgentId = agent.id;
  const options = agent.options;
  const check = useSettings((state) => state.gateways[id]);
  const dirty = useSettings(isAgentsDirty);
  const set = (patch: Partial<typeof options>) => updateOptions(id as "openrouter", patch);
  const typed = (options.apiKey ?? "").length > 0;
  const forgetting = options.clearKey === true;

  return (
    <div className="grid gap-5 sm:grid-cols-2">
      <FormField label={t("gateway.baseUrl")} htmlFor={`${id}-url`} hint={t(`gateway.baseUrl.hint.${id}` as Key)} error={problems.baseUrl && t(problems.baseUrl)}>
        <Input id={`${id}-url`} value={options.baseUrl} spellCheck={false} className="font-mono" aria-invalid={problems.baseUrl ? true : undefined}
          onChange={(event) => set({ baseUrl: event.target.value })} />
      </FormField>
      <FormField label={t("gateway.apiKey")} htmlFor={`${id}-key`} hint={t((id as string) === "litellm" ? "gateway.apiKey.hint.optional" : "gateway.apiKey.hint")} error={problems.apiKey && t(problems.apiKey)}>
        <div className="flex gap-2">
          <Input id={`${id}-key`} type="password" autoComplete="off" spellCheck={false} className="font-mono" value={options.apiKey ?? ""}
            placeholder={forgetting ? t("gateway.apiKey.removing") : options.hasKey ? t("gateway.apiKey.saved") : t("gateway.apiKey.paste")}
            aria-invalid={problems.apiKey ? true : undefined}
            onChange={(event) => set({ apiKey: event.target.value, clearKey: false })} />
          {(options.hasKey || typed) && (
            <Button variant="outline" type="button" onClick={() => set({ apiKey: "", clearKey: true })} disabled={forgetting && !typed}>{t("gateway.apiKey.clear")}</Button>
          )}
        </div>
      </FormField>
      <div className="grid gap-1.5 sm:col-span-2">
        <div className="flex flex-wrap items-center gap-3">
          <Button variant="outline" type="button" disabled={check === "checking" || dirty} title={dirty ? t("gateway.check.dirty") : undefined} onClick={() => void checkGateway(id)}>
            {t(check === "checking" ? "gateway.checking" : "gateway.check")}
          </Button>
          {dirty && <span className="text-xs text-muted-foreground">{t("gateway.check.dirty")}</span>}
        </div>
        {check && check !== "checking" && (
          check.error === null
            ? <p role="status" className="text-xs text-success">{t("gateway.check.ok", { count: check.models })}</p>
            : <p role="alert" className="text-xs text-destructive [overflow-wrap:anywhere]">{t("gateway.check.failed", { error: check.error })}</p>
        )}
      </div>
    </div>
  );
}
