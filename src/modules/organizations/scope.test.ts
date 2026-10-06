import { describe, expect, it } from "vitest";
import { filterScopeGroups, groupByScope, organizationChatOf, organizationChatsOf, splitGeneral } from "./scope";

const project = (id: string) => ({ id });

describe("groupByScope", () => {
  it("puts unlinked projects in the personal group, first", () => {
    const groups = groupByScope([project("a"), project("b")], { b: { orgId: "o1", slug: "acme", name: "Acme" } },
      [{ id: "o1", name: "Acme", slug: "acme", role: "member" }]);
    expect(groups.map((group) => group.key)).toEqual(["personal", "o1"]);
    expect(groups[0].projects.map((item) => item.id)).toEqual(["a"]);
    expect(groups[1].projects.map((item) => item.id)).toEqual(["b"]);
    expect(groups[1].role).toBe("member");
  });

  it("keeps organizations without local projects, sorted by name", () => {
    const groups = groupByScope([], {}, [
      { id: "o2", name: "Zeta", slug: "zeta", role: "owner" },
      { id: "o1", name: "Acme", slug: "acme", role: "member" },
    ]);
    expect(groups.map((group) => group.key)).toEqual(["personal", "o1", "o2"]);
    expect(groups.every((group) => group.projects.length === 0)).toBe(true);
  });

  it("creates a group from the link when the organization is not listed yet", () => {
    const groups = groupByScope([project("a")], { a: { orgId: "o9", slug: "beta", name: "Beta" } }, []);
    expect(groups[1]).toMatchObject({ key: "o9", name: "Beta", slug: "beta", role: null });
    expect(groups[0].projects).toEqual([]);
  });

  it("places the organization chat project by its own organization before the server links it", () => {
    const groups = groupByScope([{ id: "a", orgId: "o1" }, { id: "b", orgId: "gone" }], {}, [{ id: "o1", name: "Acme", slug: "acme", role: "owner" }]);
    expect(groups[1].projects.map((item) => item.id)).toEqual(["a"]);
    expect(groups[0].projects.map((item) => item.id)).toEqual(["b"]);
  });
});

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

describe("filterScopeGroups", () => {
  const item = (name: string, rootPath = `/work/${name}`, repoKeys: string[] = []) => ({ id: name, name, rootPath, repoKeys });
  const groups = groupByScope(
    [item("notes"), item("api", "/work/api", ["github.com/acme/api"]), item("web")],
    { api: { orgId: "o1", slug: "acme", name: "Acme" }, web: { orgId: "o2", slug: "zeta", name: "Zeta" } },
    [{ id: "o1", name: "Acme", slug: "acme", role: "member" }, { id: "o2", name: "Zeta", slug: "zeta", role: "owner" }],
  );

  it("keeps every group for all, only personal or only organizations otherwise", () => {
    expect(filterScopeGroups(groups, "all", "").map((group) => group.key)).toEqual(["personal", "o1", "o2"]);
    expect(filterScopeGroups(groups, "personal", "").map((group) => group.key)).toEqual(["personal"]);
    expect(filterScopeGroups(groups, "organizations", "").map((group) => group.key)).toEqual(["o1", "o2"]);
  });

  it("searches name, folder and repositories and hides groups without matches", () => {
    expect(filterScopeGroups(groups, "all", "ACME/API").map((group) => group.key)).toEqual(["o1"]);
    expect(filterScopeGroups(groups, "all", "work/notes")[0].projects.map((project) => project.id)).toEqual(["notes"]);
    expect(filterScopeGroups(groups, "organizations", "notes")).toEqual([]);
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
