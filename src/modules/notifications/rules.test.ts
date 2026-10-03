import { describe, expect, it } from "vitest";
import type { Chat, TurnStatus } from "@/modules/core";
import { emptyMemory, targetOf, timeAgo, turnEvents, type AppNotification } from "./rules";

const chat = (id: string, turns: [string, TurnStatus][], question: string | null = null): Chat => ({
  id, code: id, projectId: "p1", title: "", messages: [], createdAt: "", updatedAt: "", workMode: "auto",
  turns: turns.map(([turnId, status]) => ({ id: turnId, code: turnId, status, entry: null, exit: null, partial: null, activity: [], route: null })),
  question: question ? { turnId: question, code: "q1", kind: "single", prompt: "", options: [], source: "", status: "open" } : null,
});

const kinds = (events: { kind: string; turnId: string }[]) => events.map((event) => `${event.kind}:${event.turnId}`);

describe("turnEvents", () => {
  it("a primeira leitura só aprende", () => {
    const memory = emptyMemory();
    expect(turnEvents(memory, [chat("c1", [["t1", "answered"], ["t2", "failed"]], "t2")], true)).toEqual([]);
    expect(turnEvents(memory, [chat("c1", [["t1", "answered"], ["t2", "failed"]], "t2")], false)).toEqual([]);
  });

  it("o turno que sai da fila vira notificação, uma vez", () => {
    const memory = emptyMemory();
    turnEvents(memory, [chat("c1", [["t1", "queued"], ["t2", "flying"], ["t3", "flying"]])], true);
    const events = turnEvents(memory, [chat("c1", [["t1", "answered"], ["t2", "failed"], ["t3", "blocked"]])], false);
    expect(kinds(events)).toEqual(["turn.answered:t1", "turn.failed:t2", "turn.blocked:t3"]);
    expect(turnEvents(memory, [chat("c1", [["t1", "answered"], ["t2", "failed"], ["t3", "blocked"]])], false)).toEqual([]);
  });

  it("o turno novo que já chega fechado também conta", () => {
    const memory = emptyMemory();
    turnEvents(memory, [chat("c1", [])], true);
    expect(kinds(turnEvents(memory, [chat("c1", [["t9", "blocked"]])], false))).toEqual(["turn.blocked:t9"]);
  });

  it("o turno que termina perguntando vira só a pergunta", () => {
    const memory = emptyMemory();
    turnEvents(memory, [chat("c1", [["t1", "flying"]])], true);
    expect(kinds(turnEvents(memory, [chat("c1", [["t1", "answered"]], "t1")], false))).toEqual(["turn.asking:t1"]);
    expect(turnEvents(memory, [chat("c1", [["t1", "answered"]], "t1")], false)).toEqual([]);
  });
});

describe("targetOf", () => {
  const make = (kind: AppNotification["kind"], data: AppNotification["data"] = {}): AppNotification => ({ id: "n", kind, data, createdAt: "", read: false, local: false });
  it("cada tipo leva para o seu lugar", () => {
    expect(targetOf(make("turn.answered", { chatId: "c1" }))).toEqual({ kind: "chat", chatId: "c1" });
    expect(targetOf(make("org.invited", { orgId: "o1" }))).toEqual({ kind: "organizations" });
    expect(targetOf(make("org.roleChanged", { orgId: "o1" }))).toEqual({ kind: "organization", orgId: "o1" });
    expect(targetOf(make("org.removed", { orgId: "o1" }))).toBeNull();
    expect(targetOf(make("quota.crossed"))).toEqual({ kind: "stats" });
    expect(targetOf(make("update.available", { version: "1.2.3" }))).toEqual({ kind: "update" });
  });
});

describe("timeAgo", () => {
  it("fala no idioma pedido", () => {
    const now = Date.parse("2026-10-02T12:00:00Z");
    expect(timeAgo("2026-10-02T11:55:00Z", "en", now)).toBe("5 min. ago");
    expect(timeAgo("2026-10-01T12:00:00Z", "en", now)).toBe("yesterday");
    expect(timeAgo("2026-10-02T11:59:50Z", "pt-BR", now)).toBe("agora");
  });
});
