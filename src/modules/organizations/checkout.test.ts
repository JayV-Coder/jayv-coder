import { describe, expect, it } from "vitest";
import { localCopy, newClones } from "./checkout";

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
