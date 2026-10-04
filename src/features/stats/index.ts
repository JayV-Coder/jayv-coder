import type { FeatureManifest } from "../types";
import { StatsPage } from "./pages/StatsPage";

/** A página Estatísticas: tokens, custo e cotas por agente, modelo e
 * projeto. Os painéis de uso que o painel da organização também mostra
 * continuam em `components/organisms` até ele virar funcionalidade. */
export const statsFeature = {
  key: "stats",
  routes: { stats: StatsPage },
} satisfies FeatureManifest;
