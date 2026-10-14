import { describe, expect, it } from "vitest";
import { blockersOf, blockedCommandsOf, blockedGrantsOf, conflictsWith, grantBlockersOf } from "./commands";

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

describe("chat permissions switched off by the organization", () => {
  it("reads the project policy and ignores unknown kinds", () => {
    expect(blockedGrantsOf({ blocked_grants: ["git", "network"], grant_sources: { git: ["acme"] } }, "acme, outra")).toEqual({ git: ["acme"], network: ["acme", "outra"] });
    expect(blockedGrantsOf({ blocked_grants: ["docker", "shell"] }, "acme")).toEqual({ shell: ["acme"] });
    expect(blockedGrantsOf({ agents: null }, "acme")).toEqual({});
    expect(blockedGrantsOf(null, "")).toEqual({});
  });

  it("covers the commands each permission lets through", () => {
    expect(grantBlockersOf({ shell: ["acme"] }, "npm test")).toEqual(["acme"]);
    expect(grantBlockersOf({ git: ["acme"] }, "git add")).toEqual(["acme"]);
    expect(grantBlockersOf({ git: ["acme"] }, "npm test")).toEqual([]);
    expect(grantBlockersOf({ network: ["acme"] }, "npm install")).toEqual(["acme"]);
    expect(grantBlockersOf({ network: ["acme"] }, "curl")).toEqual(["acme"]);
    expect(grantBlockersOf({ network: ["acme"] }, "git push")).toEqual(["acme"]);
    expect(grantBlockersOf({ network: ["acme"] }, "git add")).toEqual([]);
    expect(grantBlockersOf({ shell: ["acme"], git: ["outra"] }, "git add")).toEqual(["acme", "outra"]);
  });
});
