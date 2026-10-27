import { create } from "zustand";
import { supabase } from "@/modules/auth/client";
import type { UsageBreakdown, UsageOutcomes } from "@/modules/core";

/** Um preço da tabela do site, em dólares por milhão de tokens. `pattern` é um
 * pedaço do id do modelo, em minúsculas (`sonnet`, `gpt-5`). */
export interface ModelPrice {
  pattern: string;
  inputUsd: number;
  outputUsd: number;
  cacheReadUsd: number | null;
  cacheWriteUsd: number | null;
}

interface PricesState { prices: ModelPrice[]; loaded: boolean }

export const usePrices = create<PricesState>(() => ({ prices: [], loaded: false }));

interface PriceRow { pattern: string; input_usd: number | string; output_usd: number | string; cache_read_usd: number | string | null; cache_write_usd: number | string | null }

const number = (value: number | string | null) => (value === null || value === "" ? null : Number(value));

/** Lê a tabela de preços do site. Sem rede ou sem sessão, a conta fica só com
 * o custo que as ferramentas informaram. */
export async function loadPrices() {
  const { data, error } = await supabase.from("model_prices").select("pattern, input_usd, output_usd, cache_read_usd, cache_write_usd");
  if (error) {
    usePrices.setState({ loaded: true });
    return;
  }
  const prices = ((data ?? []) as PriceRow[]).map((row) => ({
    pattern: row.pattern.toLowerCase(),
    inputUsd: number(row.input_usd) ?? 0,
    outputUsd: number(row.output_usd) ?? 0,
    cacheReadUsd: number(row.cache_read_usd),
    cacheWriteUsd: number(row.cache_write_usd),
  }));
  usePrices.setState({ prices, loaded: true });
}

/** O preço de um modelo: o padrão mais longo contido no id dele, sem
 * diferença de maiúsculas. */
export function priceFor(model: string, prices: ModelPrice[]): ModelPrice | null {
  const id = model.toLowerCase();
  let best: ModelPrice | null = null;
  for (const price of prices) if (price.pattern && id.includes(price.pattern) && (!best || price.pattern.length > best.pattern.length)) best = price;
  return best;
}

/** O dinheiro do período: o que as ferramentas informaram, o que a tabela de
 * preços estimou para o resto e os modelos que ficaram sem preço. O cache lido
 * sem preço próprio custa um décimo da entrada; o escrito, 1,25×. */
export interface MoneyEstimate { reportedUsd: number; estimatedUsd: number; totalUsd: number; unpricedModels: string[] }

export function estimateMoney(byModel: UsageBreakdown[], prices: ModelPrice[]): MoneyEstimate {
  let reportedUsd = 0;
  let estimatedUsd = 0;
  const unpricedModels: string[] = [];
  for (const row of byModel) {
    const totals = row.totals;
    reportedUsd += totals.costUsd ?? 0;
    const left = totals.unpricedInputTokens + totals.unpricedOutputTokens + totals.unpricedCacheReadTokens + totals.unpricedCacheWriteTokens;
    if (left === 0) continue;
    const model = row.label ?? row.key;
    // O Jev é pago pelo JayV, não pela conta de quem usa.
    if ((row.parent ?? "").startsWith("jev:")) continue;
    const price = priceFor(model, prices);
    if (!price) {
      unpricedModels.push(model);
      continue;
    }
    const cacheRead = price.cacheReadUsd ?? price.inputUsd * 0.1;
    const cacheWrite = price.cacheWriteUsd ?? price.inputUsd * 1.25;
    estimatedUsd += (totals.unpricedInputTokens * price.inputUsd + totals.unpricedOutputTokens * price.outputUsd
      + totals.unpricedCacheReadTokens * cacheRead + totals.unpricedCacheWriteTokens * cacheWrite) / 1_000_000;
  }
  return { reportedUsd, estimatedUsd, totalUsd: reportedUsd + estimatedUsd, unpricedModels };
}

/** Os números por tarefa: turnos por objetivo e, quando o validador comprovou
 * alguma tarefa, tokens e dinheiro por tarefa comprovada. */
export interface TaskNumbers { turnsPerObjective: number | null; tokensPerVerified: number | null; usdPerVerified: number | null; verifiedRate: number | null }

export function taskNumbers(outcomes: UsageOutcomes, money: MoneyEstimate | null): TaskNumbers {
  const judged = outcomes.verified + outcomes.failed + outcomes.unproven;
  return {
    turnsPerObjective: outcomes.objectives === 0 ? null : outcomes.turns / outcomes.objectives,
    tokensPerVerified: outcomes.verified === 0 ? null : outcomes.agentTokens / outcomes.verified,
    usdPerVerified: outcomes.verified === 0 || !money || money.totalUsd === 0 ? null : money.totalUsd / outcomes.verified,
    verifiedRate: judged === 0 ? null : outcomes.verified / judged,
  };
}
