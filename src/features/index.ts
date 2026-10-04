import { plansFeature } from "./plans";
import { statsFeature } from "./stats";
import { systemFeature } from "./system";
import type { FeatureManifest } from "./types";

export type { FeatureManifest } from "./types";
export { loadStatus } from "./system";

/** As funcionalidades que já moram em `features/`, cada uma com o próprio
 * manifesto. As outras continuam em `modules/` e `components/` até mudarem. */
export const FEATURES: FeatureManifest[] = [systemFeature, statsFeature, plansFeature];

/** As páginas que as funcionalidades desenham, pela chave da navegação. O
 * tipo guarda quais vistas são estas, então o App sabe o que falta. */
export const featurePages = { ...systemFeature.routes, ...statsFeature.routes, ...plansFeature.routes };

/** Liga todas as funcionalidades; devolve como desligar todas. */
export function connectFeatures() {
  const offs = FEATURES.flatMap((feature) => feature.connect ? [feature.connect()] : []);
  return () => offs.forEach((off) => off());
}
