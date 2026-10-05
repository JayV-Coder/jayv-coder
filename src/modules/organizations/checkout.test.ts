import { describe, expect, it } from "vitest";
import { chatReach, compareFolder, localCopy } from "./checkout";

const project = (rootPath: string, repoKeys: string[]) => ({ rootPath, repoKeys });

describe("localCopy", () => {
  it("finds the project whose remote is the repository", () => {
    const projects = [project("/code/web", ["github.com/acme/web"]), project("/code/api", ["github.com/acme/api"])];
    expect(localCopy("github.com/acme/api", projects)?.rootPath).toBe("/code/api");
    expect(localCopy("github.com/acme/other", projects)).toBeNull();
  });

  it("ignores projects without a folder on this computer", () => {
    expect(localCopy("github.com/acme/api", [project("", ["github.com/acme/api"])])).toBeNull();
  });
});

describe("compareFolder", () => {
  const repositories = [
    { repoKey: "github.com/acme/api" }, { repoKey: "github.com/acme/web" }, { repoKey: "github.com/acme/worker" },
    { repoKey: "gitlab.com/acme/docs" }, { repoKey: "github.com/acme/mobile" },
  ];

  it("splits the organization's repositories into found, missing and the folder's other clones", () => {
    const clones = [
      { path: "/code/acme/services/api", keys: ["github.com/acme/api"] },
      { path: "/code/acme/web", keys: ["github.com/acme/web"] },
      { path: "/code/acme/deep/a/b/worker", keys: ["github.com/acme/worker"] },
      { path: "/code/acme/fork", keys: ["github.com/someone/fork", "github.com/acme/fork"] },
      { path: "/code/acme/scratch", keys: [] },
    ];
    const projects = [project("/code/acme/web/", ["github.com/acme/web"]), project("/elsewhere/worker", ["github.com/acme/worker"]), project("/old/mobile", ["github.com/acme/mobile"])];
    const result = compareFolder(repositories, clones, projects);
    expect(result.found.map((item) => [item.repository.repoKey, item.state, item.elsewhere])).toEqual([
      ["github.com/acme/api", "new", null],
      ["github.com/acme/web", "project", null],
      ["github.com/acme/worker", "elsewhere", "/elsewhere/worker"],
    ]);
    expect(result.missing.map((item) => [item.repository.repoKey, item.local])).toEqual([["gitlab.com/acme/docs", null], ["github.com/acme/mobile", "/old/mobile"]]);
    expect(result.outside.map((clone) => clone.path)).toEqual(["/code/acme/fork", "/code/acme/scratch"]);
  });

  it("takes the shallowest clone when a repository is cloned twice", () => {
    const clones = [{ path: "C:\\code\\acme\\old\\copies\\api", keys: ["github.com/acme/api"] }, { path: "C:\\code\\acme\\api", keys: ["github.com/acme/api"] }];
    const result = compareFolder([{ repoKey: "github.com/acme/api" }], clones, [project("C:\\code\\acme\\api", [])]);
    expect(result.found).toEqual([{ repository: { repoKey: "github.com/acme/api" }, path: "C:\\code\\acme\\api", state: "project", elsewhere: null }]);
    expect(result.outside).toEqual([]);
  });
});

describe("chatReach", () => {
  it("splits the repositories into inside the organization folder, elsewhere and missing", () => {
    const repositories = [{ repoKey: "github.com/acme/api" }, { repoKey: "github.com/acme/web" }, { repoKey: "github.com/acme/worker" }];
    const projects = [project("/code/acme/api", ["github.com/acme/api"]), project("/other/web", ["github.com/acme/web"])];
    const reach = chatReach(repositories, projects, "/code/acme/");
    expect(reach.inside.map((item) => item.repository.repoKey)).toEqual(["github.com/acme/api"]);
    expect(reach.elsewhere.map((item) => item.path)).toEqual(["/other/web"]);
    expect(reach.missing.map((item) => item.repoKey)).toEqual(["github.com/acme/worker"]);
  });

  it("compares Windows paths and does not take a sibling folder for a child", () => {
    const repositories = [{ repoKey: "github.com/acme/api" }, { repoKey: "github.com/acme/web" }];
    const projects = [project("C:\\code\\acme\\api", ["github.com/acme/api"]), project("C:\\code\\acme-old\\web", ["github.com/acme/web"])];
    const reach = chatReach(repositories, projects, "C:\\code\\acme");
    expect(reach.inside).toHaveLength(1);
    expect(reach.elsewhere).toHaveLength(1);
  });
});

describe("organization folder", () => {
  it("is forgotten without touching other organizations", async () => {
    const store = new Map<string, string>();
    globalThis.localStorage = { getItem: (key: string) => store.get(key) ?? null, setItem: (key: string, value: string) => void store.set(key, value), removeItem: (key: string) => void store.delete(key) } as Storage;
    const { forgetOrganizationFolder, organizationFolder, rememberOrganizationFolder } = await import("./checkout");
    rememberOrganizationFolder("a", "/code/a");
    rememberOrganizationFolder("b", "/code/b");
    forgetOrganizationFolder("a");
    expect(organizationFolder("a")).toBeNull();
    expect(organizationFolder("b")).toBe("/code/b");
  });
});
