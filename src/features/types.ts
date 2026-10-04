import type { View } from "@/modules/core";

/** O que uma funcionalidade conta ao app para entrar nele: as páginas que
 * ela mostra e o que ela liga uma vez, quando o app abre. O app lê os
 * manifestos de `features/index.ts` e não precisa saber o que há dentro. */
export interface FeatureManifest {
  key: string;
  /** As vistas que a funcionalidade desenha, pela chave da navegação. */
  routes: Partial<Record<View, () => React.JSX.Element>>;
  /** Liga a funcionalidade ao núcleo e ao barramento; devolve como desligar. */
  connect?: () => () => void;
}
