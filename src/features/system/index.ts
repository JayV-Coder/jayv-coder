import type { FeatureManifest } from "../types";
import { StatusPage } from "./pages/StatusPage";
import { connectSystem } from "./store";

/** A página Sistema: a saúde do núcleo, dos agentes, da conexão e do banco
 * local, a versão e o relatório para copiar. A única porta de entrada da
 * funcionalidade; quem está fora importa só daqui. */
export { loadStatus, useSystem } from "./store";

export const systemFeature: FeatureManifest = {
  key: "system",
  routes: { status: StatusPage },
  connect: connectSystem,
};
