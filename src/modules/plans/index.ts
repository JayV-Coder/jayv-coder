import { create } from "zustand";
import type { RealtimeChannel } from "@supabase/supabase-js";
import { openUrl } from "@tauri-apps/plugin-opener";
import { supabase } from "@/modules/auth/client";
import { notify, reportError } from "@/modules/feedback";
import { t } from "@/modules/i18n";
import { FEATURES, type FeatureKey } from "./catalog";

export * from "./catalog";

/** O que vale para quem está logado. `features` nulo: a lista não desceu
 * (servidor sem a migração dos planos, sem rede) e vale tudo — ninguém perde
 * o que já usava por estar offline. */
interface EntitlementsState {
  plan: string | null;
  admin: boolean;
  features: Set<string> | null;
}

export const useEntitlements = create<EntitlementsState>(() => ({ plan: null, admin: false, features: null }));

export const allows = (state: EntitlementsState, feature: FeatureKey) => state.features === null || state.features.has(feature);

/** O recurso está no plano e ligado pelo admin. */
export function useFeature(feature: FeatureKey) {
  return useEntitlements((state) => allows(state, feature));
}

export function hasFeature(feature: FeatureKey) {
  return allows(useEntitlements.getState(), feature);
}

/** Os planos como o catálogo do admin e a página de planos os leem. */
export interface Plan {
  key: string;
  name: string;
  description: string;
  position: number;
  active: boolean;
  isDefault: boolean;
  stripePriceId: string | null;
  priceCents: number | null;
  currency: string | null;
  billingInterval: "month" | "year" | null;
  features: string[];
}

export interface FeatureRow { key: string; enabled: boolean; position: number }

export interface Subscription {
  planKey: string | null;
  status: string;
  currentPeriodEnd: string | null;
  cancelAtPeriodEnd: boolean;
}

interface CatalogState {
  loaded: boolean;
  plans: Plan[];
  features: FeatureRow[];
  subscription: Subscription | null;
  busy: string | null;
}

export const useCatalog = create<CatalogState>(() => ({ loaded: false, plans: [], features: [], subscription: null, busy: null }));

/** O erro das RPCs do admin chega como chave do i18n (`admin.plan.invalid`). */
function failure(error: { message?: string }) {
  if (error.message && /^admin\.[A-Za-z.]+$/.test(error.message)) return { key: error.message };
  return error;
}

type PlanRow = {
  key: string; name: string; description: string; position: number; active: boolean; is_default: boolean;
  stripe_price_id: string | null; price_cents: number | null; currency: string | null; billing_interval: "month" | "year" | null;
};

let channel: RealtimeChannel | null = null;

export async function loadEntitlements() {
  const { data, error } = await supabase.rpc("my_features");
  // Sem a migração (ou sem rede), fica como estava: tudo liberado na primeira
  // vez, a última lista lida depois.
  if (error) {
    console.error("plans", error);
    return;
  }
  const answer = data as { plan: string | null; admin: boolean; features: string[] };
  useEntitlements.setState({ plan: answer.plan, admin: answer.admin, features: new Set(answer.features) });
}

/** Planos, catálogo e a assinatura de quem usa. */
export async function loadCatalog() {
  const [plans, links, features, subscription] = await Promise.all([
    supabase.from("plans").select("key, name, description, position, active, is_default, stripe_price_id, price_cents, currency, billing_interval").order("position").order("key"),
    supabase.from("plan_features").select("plan_key, feature_key"),
    supabase.from("features").select("key, enabled, position").order("position").order("key"),
    supabase.from("subscriptions").select("plan_key, status, current_period_end, cancel_at_period_end").maybeSingle(),
  ]);
  for (const result of [plans, links, features]) if (result.error) throw failure(result.error);
  const included = (key: string) => (links.data ?? []).filter((link) => link.plan_key === key).map((link) => link.feature_key as string);
  const sub = subscription.data;
  useCatalog.setState({
    loaded: true,
    plans: (plans.data as PlanRow[]).map((row) => ({
      key: row.key, name: row.name, description: row.description, position: row.position, active: row.active, isDefault: row.is_default,
      stripePriceId: row.stripe_price_id, priceCents: row.price_cents, currency: row.currency, billingInterval: row.billing_interval,
      features: included(row.key),
    })),
    // Só as chaves que esta versão conhece: um recurso de uma versão mais nova
    // não tem texto aqui.
    features: (features.data as FeatureRow[]).filter((row) => (FEATURES as readonly string[]).includes(row.key)),
    subscription: sub ? { planKey: sub.plan_key, status: sub.status, currentPeriodEnd: sub.current_period_end, cancelAtPeriodEnd: sub.cancel_at_period_end } : null,
  });
}

function refresh() {
  loadEntitlements().catch((error) => console.error("plans", error));
  if (useCatalog.getState().loaded) loadCatalog().catch((error) => console.error("plans", error));
}

/** A cada login: lê o que vale e passa a ouvir a assinatura e o catálogo. */
export async function startEntitlements() {
  await loadEntitlements();
  const { data } = await supabase.auth.getUser();
  const me = data.user?.id;
  if (!me) return;
  if (channel) void supabase.removeChannel(channel);
  channel = supabase
    .channel(`plans:${me}`)
    .on("postgres_changes", { event: "*", schema: "public", table: "subscriptions", filter: `user_id=eq.${me}` }, refresh)
    .on("postgres_changes", { event: "*", schema: "public", table: "features" }, refresh)
    .on("postgres_changes", { event: "*", schema: "public", table: "plan_features" }, refresh)
    .on("postgres_changes", { event: "*", schema: "public", table: "plans" }, refresh)
    .subscribe();
}

export function clearEntitlements() {
  if (channel) void supabase.removeChannel(channel);
  channel = null;
  useEntitlements.setState({ plan: null, admin: false, features: null });
  useCatalog.setState({ loaded: false, plans: [], features: [], subscription: null, busy: null });
}

/** O link `jayv://billing/<done|cancel|portal>` com que o navegador volta do
 * Stripe. `true` quando o link era dele. */
export function receiveBillingLink(url: string) {
  let parsed: URL;
  try { parsed = new URL(url); } catch { return false; }
  if (parsed.protocol !== "jayv:" || parsed.host !== "billing") return false;
  const kind = parsed.pathname.replace(/^\/+/, "");
  if (kind === "done") notify(t("plans.paid"));
  else if (kind === "cancel") notify(t("plans.canceled"), true);
  refresh();
  return true;
}

async function billing(body: Record<string, string>) {
  const { data, error } = await supabase.functions.invoke("billing", { body });
  if (error) {
    // O corpo da recusa diz o motivo (`plan`, `config`, `stripe`...).
    const detail = await (error as { context?: Response }).context?.json?.().catch(() => null);
    throw { key: detail?.code === "config" ? "plans.error.config" : detail?.code === "plan" ? "plans.error.plan" : "plans.error.stripe" };
  }
  await openUrl((data as { url: string }).url);
}

/** Abre a página de pagamento do plano no navegador (ou o portal, para quem
 * já assina). */
export async function subscribe(plan: string) {
  useCatalog.setState({ busy: plan });
  try {
    await billing({ action: "checkout", plan });
  } catch (error) {
    reportError(error);
  } finally {
    useCatalog.setState({ busy: null });
  }
}

export async function manageSubscription() {
  useCatalog.setState({ busy: "portal" });
  try {
    await billing({ action: "portal" });
  } catch (error) {
    reportError(error);
  } finally {
    useCatalog.setState({ busy: null });
  }
}

/** O preço como a tela mostra: `R$ 49,00 / mês`, ou nada no plano gratuito. */
export function formatPrice(plan: Pick<Plan, "priceCents" | "currency">, locale: string) {
  if (plan.priceCents === null || !plan.currency) return null;
  try {
    return new Intl.NumberFormat(locale, { style: "currency", currency: plan.currency.toUpperCase() }).format(plan.priceCents / 100);
  } catch {
    return `${(plan.priceCents / 100).toFixed(2)} ${plan.currency.toUpperCase()}`;
  }
}
