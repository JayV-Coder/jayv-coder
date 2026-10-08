import { describe, expect, it } from "vitest";
import { organizationChatOf, organizationChatsOf, searchScopeGroups, splitGeneral } from "./scope";

describe("organizationChatOf", () => {
  const data = {
    projects: [
      { id: "p1", rootPath: "/work/acme", orgId: "o1" },
      { id: "p2", rootPath: "", orgId: "o2" },
      { id: "p3", rootPath: "/work/api", orgId: null },
    ],
    chats: [
      { id: "c1", projectId: "p1", updatedAt: "2026-10-01T10:00:00Z" },
      { id: "c2", projectId: "p1", updatedAt: "2026-10-02T10:00:00Z" },
      { id: "c3", projectId: "p2", updatedAt: "2026-10-03T10:00:00Z" },
      { id: "c4", projectId: "p3", updatedAt: "2026-10-03T10:00:00Z" },
    ],
  };

  it("returns the most recent chat of the organization project", () => {
    expect(organizationChatOf(data, "o1")?.id).toBe("c2");
  });

  it("ignores organization projects without a folder on this computer", () => {
    expect(organizationChatOf(data, "o2")).toBeNull();
  });

  it("returns null when the organization has no chat yet", () => {
    expect(organizationChatOf(data, "o3")).toBeNull();
  });
});

describe("searchScopeGroups", () => {
  const groups = [{
    key: "o1", scope: { kind: "organization" as const, orgId: "o1" }, name: "Acme", slug: "acme", role: "owner" as const,
    projects: [
      { id: "a", name: "Shop", rootPath: "/work/shop", repoKeys: ["github.com/acme/web"] },
      { id: "b", name: "Docs", rootPath: "/work/docs", repoKeys: [] },
    ],
  }];

  it("keeps every project when the search is empty", () => {
    expect(searchScopeGroups(groups, "  ")).toBe(groups);
  });

  it("searches name, folder and repositories and hides groups without matches", () => {
    expect(searchScopeGroups(groups, "SHOP")[0].projects.map((item) => item.id)).toEqual(["a"]);
    expect(searchScopeGroups(groups, "/work/docs")[0].projects.map((item) => item.id)).toEqual(["b"]);
    expect(searchScopeGroups(groups, "acme/web")[0].projects.map((item) => item.id)).toEqual(["a"]);
    expect(searchScopeGroups(groups, "nothing")).toEqual([]);
  });
});

describe("general chats", () => {
  const data = {
    projects: [
      { id: "g1", rootPath: "/work/acme", orgId: "o1" },
      { id: "r1", rootPath: "/work/acme/api", orgId: null },
      { id: "g2", rootPath: "", orgId: "o2" },
    ],
    chats: [
      { id: "c1", projectId: "g1", updatedAt: "2026-10-01T10:00:00Z" },
      { id: "c2", projectId: "g1", updatedAt: "2026-10-02T10:00:00Z" },
      { id: "c3", projectId: "r1", updatedAt: "2026-10-03T10:00:00Z" },
      { id: "c4", projectId: "g2", updatedAt: "2026-10-03T10:00:00Z" },
    ],
  };

  it("lists only the chats of the organization project, newest first", () => {
    expect(organizationChatsOf(data, "o1").map((chat) => chat.id)).toEqual(["c2", "c1"]);
    expect(organizationChatsOf(data, "o2")).toEqual([]);
  });

  it("splits the general project from the repository projects", () => {
    const { general, repositories } = splitGeneral(data.projects);
    expect(general.map((project) => project.id)).toEqual(["g1", "g2"]);
    expect(repositories.map((project) => project.id)).toEqual(["r1"]);
  });
});
