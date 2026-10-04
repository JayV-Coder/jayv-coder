import type { FeatureManifest } from "../types";
import { PlansPage } from "./pages/PlansPage";

/** A página Planos: o catálogo, o plano atual e a assinatura. As regras de
 * acesso (`allows`, `VIEW_FEATURE`) ficam em `modules/plans`, porque o app
 * inteiro as consulta. */
export const plansFeature = {
  key: "plans",
  routes: { plans: PlansPage },
} satisfies FeatureManifest;
