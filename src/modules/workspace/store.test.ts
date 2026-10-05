import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Chat } from "@/modules/core";

const getWorkspace = vi.fn();
const getChat = vi.fn();
vi.mock("@/modules/core", () => ({
  commands: { getWorkspace: () => getWorkspace(), getChat: (chatId: string) => getChat(chatId) },
  bus: { emit: vi.fn(), on: vi.fn(() => () => {}) },
  onCore: vi.fn(() => Promise.resolve(() => {})),
  WORK_MODES: ["auto", "plan", "build"],
}));
vi.mock("@/modules/feedback", () => ({ reportError: vi.fn() }));
vi.mock("@/modules/navigation", () => ({ navigate: vi.fn() }));

// A lateral lembra o layout no navegador; o teste roda sem ele.
vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {} });

const { ensureChat, loadWorkspace, useWorkspace } = await import("./store");

const chat = (id: string, said: string[]): Chat => ({
  id, code: id, projectId: "p1", title: id, turns: [], question: null, createdAt: "", updatedAt: "2026-10-05T00:00:00Z", workMode: "auto",
  messages: said.map((content) => ({ role: "user", content, createdAt: "", turnId: null })),
  lastPrompt: said.at(-1) ?? null, messageCount: said.length,
});
const light = (full: Chat): Chat => ({ ...full, messages: [] });

describe("workspace overview", () => {
  beforeEach(() => {
    getWorkspace.mockReset();
    getChat.mockReset();
    useWorkspace.setState({ data: { projects: [], chats: [] }, activeProjectId: null, activeChatId: null });
  });

  it("loads the open chat whole and the others light", async () => {
    const open = chat("c1", ["oi", "de novo"]);
    const other = chat("c2", ["outro"]);
    getWorkspace.mockResolvedValue({ projects: [{ id: "p1" }], chats: [light(open), light(other)] });
    getChat.mockResolvedValue(open);
    await loadWorkspace("c1");
    const chats = useWorkspace.getState().data.chats;
    expect(getChat).toHaveBeenCalledTimes(1);
    expect(chats.find((item) => item.id === "c1")?.messages).toHaveLength(2);
    expect(chats.find((item) => item.id === "c2")?.messages).toHaveLength(0);
    expect(chats.find((item) => item.id === "c2")?.lastPrompt).toBe("outro");
  });

  it("reads a light chat when it opens, and only then", async () => {
    const other = chat("c2", ["outro"]);
    useWorkspace.setState({ data: { projects: [], chats: [light(other)] } });
    getChat.mockResolvedValue(other);
    await ensureChat("c2");
    expect(useWorkspace.getState().data.chats[0].messages).toHaveLength(1);
    await ensureChat("c2");
    expect(getChat).toHaveBeenCalledTimes(1);
  });
});
