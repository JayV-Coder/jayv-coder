import { beforeEach, describe, expect, it, vi } from "vitest";
import type { McpServer } from "@/modules/core";

const saveMcpServers = vi.fn(async (servers: McpServer[]) => servers);
const checkMcpServers = vi.fn(async (servers: McpServer[]) => servers.map((server) => ({ ...server, name: server.name.trim() })));
const getMcpServers = vi.fn(async (): Promise<McpServer[]> => []);
vi.mock("@/modules/core", () => ({ commands: { saveMcpServers: (s: McpServer[]) => saveMcpServers(s), checkMcpServers: (s: McpServer[]) => checkMcpServers(s), getMcpServers: () => getMcpServers() } }));
vi.mock("@/modules/feedback", () => ({ reportError: vi.fn() }));

const { blankServer, confirmDraft, discardMcp, isMcpDirty, mcpList, openDraft, removeMcp, saveMcpChanges, setMcpEnabled, useMcp } = await import("./index");

const server = (name: string, enabled = true): McpServer => ({ ...blankServer(), name, command: "npx", enabled });

describe("MCP servers wait for Save settings", () => {
  beforeEach(() => {
    saveMcpServers.mockClear();
    getMcpServers.mockResolvedValue([server("fetch"), server("git")]);
    useMcp.setState({ servers: [server("fetch"), server("git")], pending: null, draft: null });
  });

  it("toggling and removing change only the pending list", () => {
    setMcpEnabled("git", false);
    removeMcp("fetch");
    expect(mcpList(useMcp.getState())?.map((item) => [item.name, item.enabled])).toEqual([["git", false]]);
    expect(isMcpDirty(useMcp.getState())).toBe(true);
    expect(useMcp.getState().servers).toHaveLength(2);
    expect(saveMcpServers).not.toHaveBeenCalled();
  });

  it("going back to what is saved is not a change", () => {
    setMcpEnabled("git", false);
    setMcpEnabled("git", true);
    expect(isMcpDirty(useMcp.getState())).toBe(false);
  });

  it("the dialog applies into the pending list; editing a renamed server replaces it", async () => {
    openDraft([server("git-renamed")], false, "git");
    expect(await confirmDraft([server(" git-renamed ")])).toBe(true);
    expect(useMcp.getState().draft).toBeNull();
    expect(mcpList(useMcp.getState())?.map((item) => item.name)).toEqual(["fetch", "git-renamed"]);
    expect(saveMcpServers).not.toHaveBeenCalled();
  });

  it("a refused server keeps the dialog open and the list untouched", async () => {
    checkMcpServers.mockRejectedValueOnce(new Error("mcp.invalid.command"));
    openDraft([server("bad")]);
    expect(await confirmDraft([server("bad")])).toBe(false);
    expect(useMcp.getState().draft).not.toBeNull();
    expect(useMcp.getState().pending).toBeNull();
  });

  it("Save stores the pending list and Discard drops it", async () => {
    removeMcp("fetch");
    expect(await saveMcpChanges()).toBe(true);
    expect(saveMcpServers).toHaveBeenCalledWith([server("git")]);
    expect(useMcp.getState().pending).toBeNull();
    setMcpEnabled("git", false);
    discardMcp();
    expect(useMcp.getState().pending).toBeNull();
  });
});
