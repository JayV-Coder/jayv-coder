import { describe, expect, it } from "vitest";
import type { UsageBreakdown, UsageTotals } from "@/modules/core";
import { estimateMoney, priceFor, taskNumbers, type ModelPrice } from "./prices";

const totals = (over: Partial<UsageTotals>): UsageTotals => ({
  inputTokens: 0, outputTokens: 0, cacheReadTokens: 0, cacheWriteTokens: 0, costUsd: null, requests: null, calls: 1, failures: 0, turns: 1,
  durationMs: 0, estimatedTokens: 0, unpricedInputTokens: 0, unpricedOutputTokens: 0, unpricedCacheReadTokens: 0, unpricedCacheWriteTokens: 0, ...over,
});
const row = (parent: string, model: string, over: Partial<UsageTotals>): UsageBreakdown => ({ key: `${parent}/${model}`, label: model, parent, totals: totals(over) });
const prices: ModelPrice[] = [
  { pattern: "gpt", inputUsd: 1, outputUsd: 2, cacheReadUsd: null, cacheWriteUsd: null },
  { pattern: "gpt-5-mini", inputUsd: 0.5, outputUsd: 1, cacheReadUsd: 0.01, cacheWriteUsd: null },
];

describe("model prices", () => {
  it("picks the longest pattern inside the model id", () => {
    expect(priceFor("GPT-5-mini-2026", prices)?.pattern).toBe("gpt-5-mini");
    expect(priceFor("gpt-5", prices)?.pattern).toBe("gpt");
    expect(priceFor("sonnet", prices)).toBeNull();
  });

  it("adds the reported cost to the estimate of what was left without one", () => {
    const money = estimateMoney([
      row("claude", "sonnet", { costUsd: 0.5 }),
      row("codex", "gpt-5", { unpricedInputTokens: 1_000_000, unpricedOutputTokens: 500_000, unpricedCacheReadTokens: 1_000_000 }),
      row("copilot", "unknown-model", { unpricedInputTokens: 10 }),
      row("jev:entry", "jev-latest", { unpricedInputTokens: 10 }),
    ], prices);
    expect(money.reportedUsd).toBe(0.5);
    expect(money.estimatedUsd).toBeCloseTo(1 + 1 + 0.1);
    expect(money.totalUsd).toBeCloseTo(2.6);
    expect(money.unpricedModels).toEqual(["unknown-model"]);
  });

  it("gives per-task numbers only when there is something to divide by", () => {
    const none = taskNumbers({ objectives: 0, turns: 0, verified: 0, failed: 0, unproven: 0, agentTokens: 0 }, null);
    expect(none).toEqual({ turnsPerObjective: null, tokensPerVerified: null, usdPerVerified: null, verifiedRate: null });
    const some = taskNumbers({ objectives: 2, turns: 5, verified: 2, failed: 1, unproven: 1, agentTokens: 1000 }, { reportedUsd: 1, estimatedUsd: 1, totalUsd: 2, unpricedModels: [] });
    expect(some).toEqual({ turnsPerObjective: 2.5, tokensPerVerified: 500, usdPerVerified: 1, verifiedRate: 0.5 });
  });
});
