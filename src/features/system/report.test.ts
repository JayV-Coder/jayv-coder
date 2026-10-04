import { describe, expect, it } from "vitest";
import { agentHealth, connectionHealth, diagnosticReport, tableSummary } from "./report";

const status = {
  version: "0.39.0", config_path: "/home/a/.jayv/config.json", database_path: "/home/a/.jayv/jayv.db", database_name: "jayv.db",
  tables: [{ name: "chats", rows: 4 }, { name: "messages", rows: 40 }, { name: "projects", rows: 4 }],
  providers: 2, models: 5, indexed_files: 120, cache_entries: 3, session_messages: 9, performance_records: 7,
};

describe("connectionHealth", () => {
  it("is ok only online with nothing pending", () => {
    expect(connectionHealth("online", 0, 0)).toBe("ok");
    expect(connectionHealth("online", 2, 0)).toBe("warn");
    expect(connectionHealth("offline", 0, 0)).toBe("warn");
    expect(connectionHealth("expired", 0, 0)).toBe("warn");
    expect(connectionHealth("signedOut", 0, 0)).toBe("off");
  });

  it("fails when the server refused a change", () => {
    expect(connectionHealth("online", 0, 1)).toBe("fail");
  });
});

describe("agentHealth", () => {
  it("tells found, silent, missing and not configured apart", () => {
    expect(agentHealth({ path: "/bin/claude", version: "2.0" })).toBe("ok");
    expect(agentHealth({ path: "/bin/claude", version: null })).toBe("warn");
    expect(agentHealth({ path: null, version: null })).toBe("fail");
    expect(agentHealth(null)).toBe("off");
  });
});

describe("tableSummary", () => {
  it("sorts by rows, then by name, and sums them", () => {
    const summary = tableSummary(status.tables);
    expect(summary.sorted.map((table) => table.name)).toEqual(["messages", "chats", "projects"]);
    expect(summary.total).toBe(48);
    expect(summary.largest).toBe(40);
  });

  it("handles an empty database", () => {
    expect(tableSummary([])).toEqual({ sorted: [], total: 0, largest: 0 });
  });
});

describe("diagnosticReport", () => {
  it("lists version, connection, agents, numbers and tables", () => {
    const text = diagnosticReport({
      status, link: "online", pending: 0, refused: 0, platform: "Linux", language: "pt-BR", at: new Date("2026-10-03T12:00:00Z"),
      agents: [
        { label: "Claude Code", probe: { path: "/bin/claude", version: "2.0" } },
        { label: "Codex", probe: { path: null, version: null } },
        { label: "Cursor", probe: null },
      ],
    });
    expect(text.split("\n")[0]).toBe("JayV 0.39.0");
    expect(text).toContain("Connection: online, 0 pending, 0 refused");
    expect(text).toContain("- Claude Code: /bin/claude (2.0)");
    expect(text).toContain("- Codex: not found");
    expect(text).toContain("- Cursor: not configured");
    expect(text).toContain("Router records: 7");
    expect(text.indexOf("- messages: 40")).toBeLessThan(text.indexOf("- chats: 4"));
  });
});
