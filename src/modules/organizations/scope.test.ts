import { describe, expect, it } from "vitest";
import { groupByScope } from "./scope";

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
});
