import { useEffect, useState } from "react";
import { reportError } from "@/modules/feedback";
import { useT, type Key } from "@/modules/i18n";
import { deletePlan, loadCatalog, savePlan, setFeatureEnabled, useCatalog, useEntitlements, type FeatureRow, type Plan } from "@/modules/plans";
import { EmptyText, LoadingNote } from "@/components/atoms";
import { ConfirmAction, FormField, OptionSelect, PageHeading, SettingsSection, ToggleRow } from "@/components/molecules";
import { ScrollPage } from "@/components/templates";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

const blankPlan = (position: number): Plan => ({
  key: "", name: "", description: "", position, active: true, isDefault: false,
  stripePriceId: null, priceCents: null, currency: "brl", billingInterval: "month", features: [],
});

/** A administração do sistema: liga e desliga cada recurso para todo mundo e
 * monta os planos (preço do Stripe e recursos de cada um). Só aparece para
 * quem está em `system_admins`; o servidor confere de novo em cada gravação. */
export function AdminPage() {
  const t = useT();
  const admin = useEntitlements((state) => state.admin);
  const { loaded, plans, features, subscribers } = useCatalog();
  const [adding, setAdding] = useState(false);

  useEffect(() => { if (admin) loadCatalog().catch(reportError); }, [admin]);
  if (!admin) return <EmptyText>{t("admin.forbidden")}</EmptyText>;
  if (!loaded) return <LoadingNote>{t("plans.loading")}</LoadingNote>;

  return (
    <ScrollPage>
      <PageHeading eyebrow={t("admin.eyebrow")} title={t("nav.admin")} description={t("admin.description")} />
      <Tabs defaultValue="features" className="gap-5">
        <TabsList className="h-auto w-full justify-start gap-1 p-1">
          <TabsTrigger value="features" className="flex-none px-4 py-2">{t("admin.tab.features")}</TabsTrigger>
          <TabsTrigger value="plans" className="flex-none px-4 py-2">{t("admin.tab.plans")}</TabsTrigger>
        </TabsList>

        <TabsContent value="features">
          <SettingsSection title={t("admin.features.title")} description={t("admin.features.description")}>
            <div className="grid gap-2">
              {features.map((feature) => (
                <ToggleRow
                  key={feature.key}
                  id={`feature-${feature.key}`}
                  label={t(`feature.${feature.key}.title` as Key)}
                  hint={t(`feature.${feature.key}.detail` as Key)}
                  checked={feature.enabled}
                  onChange={(enabled) => void setFeatureEnabled(feature.key, enabled)}
                />
              ))}
            </div>
          </SettingsSection>
        </TabsContent>

        <TabsContent value="plans" className="grid gap-4">
          <p className="text-sm text-muted-foreground">{t("admin.plans.description")}</p>
          {plans.map((plan) => (
            <PlanEditor key={plan.key} plan={plan} features={features} subscribers={subscribers[plan.key] ?? 0} />
          ))}
          {adding
            ? <PlanEditor plan={blankPlan((plans.at(-1)?.position ?? 0) + 10)} features={features} subscribers={0} fresh onDone={() => setAdding(false)} />
            : <Button variant="outline" className="justify-self-start" onClick={() => setAdding(true)}>{t("admin.plan.new")}</Button>}
        </TabsContent>
      </Tabs>
    </ScrollPage>
  );
}

function PlanEditor({ plan, features, subscribers, fresh, onDone }: {
  plan: Plan; features: FeatureRow[]; subscribers: number; fresh?: boolean; onDone?: () => void;
}) {
  const t = useT();
  const busy = useCatalog((state) => state.busy);
  const [draft, setDraft] = useState(plan);
  // O catálogo relido (outra aba, outro admin) troca o rascunho sem alteração.
  useEffect(() => setDraft(plan), [plan]);
  const dirty = fresh || JSON.stringify(draft) !== JSON.stringify(plan);
  const edit = (patch: Partial<Plan>) => setDraft((current) => ({ ...current, ...patch }));
  const toggle = (key: string, on: boolean) => edit({ features: on ? [...draft.features, key] : draft.features.filter((item) => item !== key) });
  const id = (field: string) => `plan-${plan.key || "new"}-${field}`;
  const price = draft.priceCents === null ? "" : (draft.priceCents / 100).toString();
  const paid = !draft.isDefault;

  const save = async () => {
    if (await savePlan({ ...draft, stripePriceId: paid ? draft.stripePriceId : null })) onDone?.();
  };

  return (
    <SettingsSection
      title={draft.name || t("admin.plan.untitled")}
      description={fresh ? t("admin.plan.newHint") : t("admin.plan.subscribers", { count: subscribers })}
      action={(
        <div className="flex items-center gap-2">
          {draft.isDefault && <Badge variant="accent">{t("admin.plan.defaultBadge")}</Badge>}
          {!draft.active && <Badge variant="secondary">{t("admin.plan.inactiveBadge")}</Badge>}
        </div>
      )}
    >
      <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-4">
        <FormField label={t("admin.plan.key")} htmlFor={id("key")} hint={t("admin.plan.keyHint")}>
          <Input id={id("key")} value={draft.key} disabled={!fresh} onChange={(event) => edit({ key: event.target.value.toLowerCase() })} />
        </FormField>
        <FormField label={t("admin.plan.name")} htmlFor={id("name")}>
          <Input id={id("name")} value={draft.name} maxLength={60} onChange={(event) => edit({ name: event.target.value })} />
        </FormField>
        <FormField label={t("admin.plan.descriptionField")} htmlFor={id("description")} wide>
          <Input id={id("description")} value={draft.description} maxLength={300} onChange={(event) => edit({ description: event.target.value })} />
        </FormField>
        {paid && (
          <>
            <FormField label={t("admin.plan.price")} htmlFor={id("price")} hint={t("admin.plan.priceHint")}>
              <Input id={id("price")} type="number" min={0} step="0.01" value={price} onChange={(event) => edit({ priceCents: event.target.value === "" ? null : Math.round(Number(event.target.value) * 100) })} />
            </FormField>
            <FormField label={t("admin.plan.currency")} htmlFor={id("currency")}>
              <Input id={id("currency")} value={draft.currency ?? ""} maxLength={3} onChange={(event) => edit({ currency: event.target.value.toLowerCase() || null })} />
            </FormField>
            <FormField label={t("admin.plan.interval")} htmlFor={id("interval")}>
              <OptionSelect
                id={id("interval")}
                value={draft.billingInterval ?? "month"}
                options={[{ value: "month", label: t("plans.interval.month") }, { value: "year", label: t("plans.interval.year") }]}
                onChange={(billingInterval) => edit({ billingInterval })}
              />
            </FormField>
            <FormField label={t("admin.plan.stripePrice")} htmlFor={id("stripe")} hint={t("admin.plan.stripePriceHint")} wide>
              <Input id={id("stripe")} value={draft.stripePriceId ?? ""} placeholder="price_..." onChange={(event) => edit({ stripePriceId: event.target.value.trim() || null })} />
            </FormField>
          </>
        )}
      </div>

      <div className="grid gap-2 sm:grid-cols-2">
        <ToggleRow id={id("active")} label={t("admin.plan.active")} hint={t("admin.plan.activeHint")} checked={draft.active} disabled={draft.isDefault} onChange={(active) => edit({ active })} />
        <ToggleRow id={id("default")} label={t("admin.plan.default")} hint={t("admin.plan.defaultHint")} checked={draft.isDefault} disabled={plan.isDefault} onChange={(isDefault) => edit({ isDefault, active: isDefault || draft.active })} />
      </div>

      <fieldset className="grid gap-2">
        <legend className="mb-1 text-xs text-muted-foreground">{t("admin.plan.features")}</legend>
        <div className="grid gap-2 sm:grid-cols-2">
          {features.map((feature) => (
            <label key={feature.key} className="flex items-center gap-2.5 text-sm">
              <Checkbox checked={draft.features.includes(feature.key)} onCheckedChange={(on) => toggle(feature.key, on === true)} />
              <span className={feature.enabled ? undefined : "text-muted-foreground line-through"}>{t(`feature.${feature.key}.title` as Key)}</span>
            </label>
          ))}
        </div>
      </fieldset>

      <div className="flex flex-wrap justify-end gap-2">
        {fresh
          ? <Button variant="ghost" onClick={onDone}>{t("settings.discard")}</Button>
          : !plan.isDefault && (
            <ConfirmAction
              title={t("admin.plan.deleteTitle", { plan: plan.name })}
              description={t("admin.plan.deleteHint")}
              confirm={t("admin.plan.delete")}
              onConfirm={() => deletePlan(plan)}
            >
              <Button variant="ghost" disabled={busy !== null}>{t("admin.plan.delete")}</Button>
            </ConfirmAction>
          )}
        {!fresh && dirty && <Button variant="ghost" onClick={() => setDraft(plan)}>{t("settings.discard")}</Button>}
        <Button disabled={!dirty || busy !== null || !draft.key || !draft.name.trim()} onClick={() => void save()}>{t("settings.save")}</Button>
      </div>
    </SettingsSection>
  );
}
