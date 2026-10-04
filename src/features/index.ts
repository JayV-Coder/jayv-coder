import type { View } from "@/modules/core";
import { systemFeature } from "./system";
import type { FeatureManifest } from "./types";

export type { FeatureManifest } from "./types";
export { loadStatus } from "./system";

/** As funcionalidades que já moram em `features/`, cada uma com o próprio
 * manifesto. As outras continuam em `modules/` e `components/` até mudarem. */
export const FEATURES: FeatureManifest[] = [systemFeature];

/** As páginas que as funcionalidades desenham, pela chave da navegação. */
export const featurePages = Object.assign({}, ...FEATURES.map((feature) => feature.routes)) as Partial<Record<View, () => React.JSX.Element>>;

/** Liga todas as funcionalidades; devolve como desligar todas. */
export function connectFeatures() {
  const offs = FEATURES.flatMap((feature) => feature.connect ? [feature.connect()] : []);
  return () => offs.forEach((off) => off());
}
