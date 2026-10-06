import { describe, expect, it } from "vitest";
import { mcpCommand, pairs, pairText } from "./text";

describe("mcp", () => {
  it("reads /mcp and what comes after it", () => {
    expect(mcpCommand("/mcp")).toBe("");
    expect(mcpCommand("/mcp quero o do GitHub")).toBe("quero o do GitHub");
    expect(mcpCommand("/mcp {\n\"mcpServers\": {}\n}")).toBe("{\n\"mcpServers\": {}\n}");
    expect(mcpCommand("/mcpx")).toBeNull();
    expect(mcpCommand("fale do /mcp")).toBeNull();
  });

  it("turns lines into pairs and back", () => {
    expect(pairs("TOKEN=abc\n\nURL = x=y", "=")).toEqual({ TOKEN: "abc", URL: "x=y" });
    expect(pairs("Authorization: Bearer t", ":")).toEqual({ Authorization: "Bearer t" });
    expect(pairText({ A: "1" }, "=")).toBe("A=1");
    expect(pairText({ A: "1" }, ":")).toBe("A: 1");
  });
});
