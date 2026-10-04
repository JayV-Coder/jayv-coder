import { useEffect } from "react";
import { CheckIcon, MinusIcon } from "lucide-react";
import { reportError } from "@/modules/feedback";
import { useLocale, useT, type Key } from "@/modules/i18n";
import { formatPrice, loadCatalog, manageSubscription, subscribe, useCatalog, useEntitlements } from "@/modules/plans";
import { LoadingNote } from "@/components/atoms";
import { PageHeading } from "@/components/molecules";
import { ScrollPage } from "@/components/templates";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";

/** Os planos à venda, o que cada um traz e a assinatura de quem usa. O
 * pagamento abre no navegador, no Stripe; a volta reabre o app. */
export function PlansPage() {
  const t = useT();
  const locale = useLocale();
  const { loaded, plans, features, subscription, busy } = useCatalog();
  const current = useEntitlements((state) => state.plan);

  useEffect(() => { loadCatalog().catch(reportError); }, []);
  if (!loaded) return <LoadingNote>{t("plans.loading")}</LoadingNote>;

  const shown = plans.filter((plan) => plan.active || plan.key === current);
  const enabled = features.filter((feature) => feature.enabled);
  const day = (iso: string) => new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(new Date(iso));
  const paying = subscription !== null && ["active", "trialing", "past_due"].includes(subscription.status);

  return (
    <ScrollPage>
      <PageHeading eyebrow={t("header.account")} title={t("nav.plans")} description={t("plans.description")}>
        {subscription && <Button variant="ghost" disabled={busy === "portal"} onClick={() => void manageSubscription()}>{t("plans.manage")}</Button>}
      </PageHeading>

      {paying && subscription && (
        <p className={cn("mb-5 rounded-lg border px-4 py-2.5 text-sm", subscription.status === "past_due" ? "border-warning/40 bg-warning/10 text-warning" : "border-border bg-card")}>
          {t(`plans.status.${subscription.status}` as Key)}
          {subscription.currentPeriodEnd && ` ${t(subscription.cancelAtPeriodEnd ? "plans.endsOn" : "plans.renewsOn", { date: day(subscription.currentPeriodEnd) })}`}
        </p>
      )}

      <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-4">
        {shown.map((plan) => {
          const price = formatPrice(plan, locale);
          const mine = plan.key === current;
          return (
            <Card key={plan.key} className={cn("gap-4 px-5 py-5", mine && "border-primary")}>
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <h3 className="text-base font-semibold break-words">{plan.name}</h3>
                  {plan.description && <p className="mt-0.5 text-xs text-muted-foreground">{plan.description}</p>}
                </div>
                {mine && <Badge variant="success">{t("plans.current")}</Badge>}
              </div>
              <p className="font-mono text-h3 font-semibold">
                {price ?? t("plans.free")}
                {price && plan.billingInterval && <span className="text-xs font-normal text-muted-foreground"> / {t(`plans.interval.${plan.billingInterval}`)}</span>}
              </p>
              <ul className="grid gap-1.5">
                {enabled.map((feature) => {
                  const included = plan.features.includes(feature.key);
                  return (
                    <li key={feature.key} className={cn("flex items-center gap-2 text-sm", !included && "text-muted-foreground/70")}>
                      {included ? <CheckIcon aria-hidden="true" className="size-4 text-go" /> : <MinusIcon aria-hidden="true" className="size-4" />}
                      <span>{t(`feature.${feature.key}.title` as Key)}</span>
                      <span className="sr-only">{t(included ? "plans.included" : "plans.notIncluded")}</span>
                    </li>
                  );
                })}
              </ul>
              {!mine && plan.stripePriceId && (
                <Button className="mt-auto" disabled={busy !== null} onClick={() => void (paying ? manageSubscription() : subscribe(plan.key))}>
                  {t(paying ? "plans.change" : "plans.subscribe")}
                </Button>
              )}
            </Card>
          );
        })}
      </div>
    </ScrollPage>
  );
}
