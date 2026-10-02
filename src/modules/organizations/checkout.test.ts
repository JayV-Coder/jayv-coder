import { describe, expect, it } from "vitest";
import { chatReach, localCopy, newClones } from "./checkout";

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

describe("newClones", () => {
  it("leaves out folders and repositories that are already projects", () => {
    const found = [
      { key: "github.com/acme/api", path: "/code/acme/api" },
      { key: "github.com/acme/web", path: "/code/acme/web/" },
      { key: "github.com/acme/worker", path: "/code/acme/worker" },
    ];
    const projects = [project("/code/acme/web", []), project("/elsewhere/worker", ["github.com/acme/worker"])];
    expect(newClones(found, projects).map((clone) => clone.key)).toEqual(["github.com/acme/api"]);
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
