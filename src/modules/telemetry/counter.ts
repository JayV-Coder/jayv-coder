/** O que o app mede por conta própria: a tela, a aba ou o painel de cada
 * recurso aberto. As portarias, a proteção, a sessão, o cache e a memória o
 * servidor tira das marcas do Jev (`feature_of_jev_kind`); a lista das duas
 * fontes é a `telemetry_measured()` da migração da 0.110.0. */
export const MEASURED = [
  "organizations", "orgChat", "stats", "gateBoard", "liveFiles", "chatSearch", "conversationFind", "mcp", "skills",
  "skillsHub", "kiloCode", "gatewayProviders", "customMods",
] as const;

export type MeasuredFeature = (typeof MEASURED)[number];

export interface UsageItem { feature: string; day: string; uses: number }

/** As contagens ainda não enviadas, por recurso e dia (UTC). */
export class Counter {
  private counts = new Map<string, number>();

  add(feature: MeasuredFeature, now: Date = new Date()) {
    const key = `${feature}|${now.toISOString().slice(0, 10)}`;
    this.counts.set(key, Math.min((this.counts.get(key) ?? 0) + 1, 99_999));
  }

  get size() {
    return this.counts.size;
  }

  /** Esvazia o contador e devolve o que havia, pronto para a RPC. */
  drain(): UsageItem[] {
    const items = [...this.counts].map(([key, uses]) => {
      const [feature, day] = key.split("|");
      return { feature, day, uses };
    });
    this.counts.clear();
    return items;
  }

  /** Devolve o que não foi enviado, somando ao que entrou depois. */
  restore(items: UsageItem[]) {
    for (const item of items) {
      const key = `${item.feature}|${item.day}`;
      this.counts.set(key, Math.min((this.counts.get(key) ?? 0) + item.uses, 99_999));
    }
  }
}
