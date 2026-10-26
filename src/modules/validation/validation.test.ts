import { describe, expect, it, vi } from "vitest";
import type { Activity } from "@/modules/core";

// O i18n lê o idioma guardado ao carregar; no teste não há navegador.
vi.hoisted(() => {
  const stored = new Map<string, string>();
  globalThis.localStorage = { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => void stored.set(key, value) } as Storage;
});

const { messageLight } = await import("@/modules/conversation");
const { lacksTest, stepDuration, validationOf } = await import("./index");

let seq = 0;
const event = (kind: string, detail: Record<string, unknown> = {}): Activity => ({ seq: ++seq, kind, detail } as unknown as Activity);
const step = (kind: string, status: string, extra: Record<string, unknown> = {}) => event("validate_step", { attempt: 0, step: kind, command: `run ${kind}`, status, exitCode: status === "passed" ? 0 : 1, millis: 1200, tail: "", blockedBy: [], ...extra });
const start = (kinds: string[], undef: string[] = []) => event("validate_start", { attempt: 0, maxRepairs: 2, steps: kinds.map((kind) => ({ kind, command: `run ${kind}` })), undefined: undef });

describe("validationOf", () => {
  it("is null for a turn that never built", () => {
    expect(validationOf([event("route"), event("done")])).toBeNull();
  });

  it("follows a round from planned to passed", () => {
    const running = validationOf([event("checkpoint", { id: "c1", commit: "abc", head: "def" }), start(["test", "lint"])]);
    expect(running?.verdict).toBeNull();
    expect(running?.steps.map((s) => s.status)).toEqual(["pending", "pending"]);
    const done = validationOf([start(["test", "lint"]), step("test", "passed"), step("lint", "passed"), event("validate_end", { attempt: 0, verdict: "passed" })]);
    expect(done?.verdict).toBe("passed");
    expect(done?.steps.every((s) => s.status === "passed")).toBe(true);
  });

  it("keeps only the last round and counts repairs", () => {
    const state = validationOf([
      start(["test"]), step("test", "failed", { tail: "boom" }), event("validate_end", { verdict: "failed" }),
      event("repair", { attempt: 1, max: 2 }),
      start(["test"]), step("test", "passed"), event("validate_end", { verdict: "passed" }),
    ]);
    expect(state?.repairs).toBe(1);
    expect(state?.maxRepairs).toBe(2);
    expect(state?.steps).toHaveLength(1);
    expect(state?.steps[0].status).toBe("passed");
  });

  it("never calls a project without a test passed", () => {
    const state = validationOf([start(["lint"], ["test"]), step("lint", "passed"), event("validate_end", { verdict: "unproven" })]);
    expect(state && lacksTest(state)).toBe(true);
    expect(messageLight("assistant", { status: "answered", entry: null, exit: null, validation: [start(["lint"], ["test"]), step("lint", "passed"), event("validate_end", { verdict: "unproven" })] })?.label).toBe("verdict.unverified");
  });

  it("records the rollback and the held commit", () => {
    const state = validationOf([event("checkpoint", { id: "c1", commit: "abc" }), event("rolled_back", { files: 3, headMoved: true }), event("commit_held", { commands: ["git commit"], orgs: ["a"] })]);
    expect(state?.rolledBack).toEqual({ files: 3, headMoved: true });
    expect(state?.held?.commands).toEqual(["git commit"]);
  });
});

describe("messageLight with validation", () => {
  const turn = (validation: Activity[]) => ({ status: "answered" as const, entry: null, exit: null, validation });
  it("is green only when the tests passed", () => {
    const ok = [start(["test"]), step("test", "passed"), event("validate_end", { verdict: "passed" })];
    expect(messageLight("assistant", turn(ok))).toEqual({ aspect: "go", label: "verdict.validated" });
  });
  it("is red when validation failed", () => {
    const bad = [start(["test"]), step("test", "failed"), event("validate_end", { verdict: "failed" })];
    expect(messageLight("assistant", turn(bad))).toEqual({ aspect: "stop", label: "verdict.validationFailed" });
  });
  it("stays unverified without validation", () => {
    expect(messageLight("assistant", turn([]))?.label).toBe("verdict.unverified");
  });
});

describe("stepDuration", () => {
  it("speaks in short units", () => {
    expect(stepDuration(450)).toBe("450 ms");
    expect(stepDuration(2500)).toBe("2.5 s");
    expect(stepDuration(125000)).toBe("2 min 5 s");
  });
});
