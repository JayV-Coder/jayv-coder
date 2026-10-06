import { describe, expect, it } from "vitest";
import { blockersOf, blockedCommandsOf, conflictsWith } from "./commands";

const blocked = { "git push": ["acme"], docker: ["acme", "outra"] };

describe("blocked commands", () => {
  it("names who blocks a command by word prefix", () => {
    expect(blockersOf(blocked, "git push origin main")).toEqual(["acme"]);
    expect(blockersOf(blocked, "docker compose up")).toEqual(["acme", "outra"]);
    expect(blockersOf(blocked, "git pull")).toEqual([]);
    expect(blockersOf(blocked, "dockerize")).toEqual([]);
  });

  it("stops a broader grant from covering a blocked command", () => {
    expect(conflictsWith(blocked, "git")).toEqual(["acme"]);
    expect(conflictsWith(blocked, "git push")).toEqual(["acme"]);
    expect(conflictsWith(blocked, "git add")).toEqual([]);
    expect(conflictsWith(blocked, "docker run")).toEqual(["acme", "outra"]);
  });

  it("reads the project policy", () => {
    expect(blockedCommandsOf({ blocked_commands: ["git push", "make"], command_sources: { "git push": ["acme"] } }, "acme, outra")).toEqual({ "git push": ["acme"], make: ["acme", "outra"] });
    expect(blockedCommandsOf({ blocked_commands: ["rm"] }, "acme")).toEqual({ rm: ["acme"] });
    expect(blockedCommandsOf({ agents: null }, "acme")).toEqual({});
    expect(blockedCommandsOf(null, "")).toEqual({});
  });
});
