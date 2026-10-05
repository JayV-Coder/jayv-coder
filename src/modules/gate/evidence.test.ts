import { describe, expect, it, vi } from "vitest";
import type { TurnEvidence } from "@/modules/core";

// O i18n lê o idioma guardado ao carregar; no teste não há navegador.
vi.hoisted(() => {
  const stored = new Map<string, string>();
  globalThis.localStorage = { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => void stored.set(key, value) } as Storage;
});

const { t } = await import("@/modules/i18n");
const { evidenceGroups } = await import("./evidence");

const checked: TurnEvidence = {
  entry: { score: 78, demand: 55, verdict: "pass", scopeLevel: 1, doneCriterion: true },
  exits: [
    { kind: "command", target: "cargo test", rule: null, verdict: "cleared" },
    { kind: "command", target: "rm -rf build", rule: "permissions.shell · deny", verdict: "held" },
    { kind: "file", target: "src/lib.rs", rule: null, verdict: "cleared" },
  ],
  review: { provider: "codex", model: "gpt-5.5", files: 2, arrived: true },
};

describe("evidenceGroups", () => {
  it("puts the gates under what JayV observed, with each held item and its rule", () => {
    const { observed } = evidenceGroups(checked, t);
    expect(observed[0]).toContain("78 out of 100");
    expect(observed[0]).toContain("feature");
    expect(observed).toContain("Exit gate: 2 commands in the answer were checked against the house rules.");
    expect(observed).toContain("Exit gate: 1 file in the answer was checked against the house rules.");
    expect(observed).toContain("Held by permissions.shell · deny: rm -rf build (command).");
  });

  it("never lists the agent's answer or the second opinion as observed", () => {
    const { observed, inferred } = evidenceGroups(checked, t);
    expect(inferred[0]).toContain("not checked by JayV");
    expect(inferred[1]).toContain("Codex");
    expect(inferred[1]).toContain("not a test");
    expect(observed.join(" ")).not.toContain("Codex");
  });

  it("always says that no test, build or lint ran, and that the done criterion was not checked", () => {
    const { unverified } = evidenceGroups(checked, t);
    expect(unverified).toEqual([
      "JayV ran no test, build or lint in this turn.",
      "The request says how to know it is done, but JayV did not check that criterion.",
    ]);
  });

  it("says when the answer had nothing for the exit gate and the review never arrived", () => {
    const bare = evidenceGroups({ entry: { ...checked.entry!, doneCriterion: false }, exits: [], review: { ...checked.review!, arrived: false } }, t);
    expect(bare.observed).toContain("Exit gate: the answer named no command to run and no file to change.");
    expect(bare.inferred[1]).toContain("has not arrived");
    expect(bare.unverified[1]).toContain("did not say how to know it is done");
  });

  it("reads the exit kinds recorded before the switch to English identifiers", () => {
    const legacy = evidenceGroups({ entry: null, exits: [{ kind: "comando", target: "make", rule: "permissions.shell · ask", verdict: "held" }], review: null }, t);
    expect(legacy.observed).toEqual([
      "Exit gate: 1 command in the answer was checked against the house rules.",
      "Held by permissions.shell · ask: make (command).",
    ]);
    expect(legacy.unverified).toEqual(["JayV ran no test, build or lint in this turn."]);
  });
});
