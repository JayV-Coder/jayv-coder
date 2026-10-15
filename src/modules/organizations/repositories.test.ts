import { describe, expect, it } from "vitest";
import type { RepositoryState } from "@/modules/core";
import { emptyPolicy, type StoredPolicy } from "./policy";
import { chatRepositories, repositoryLabel, safeDirectoryCommand } from "./repositories";

const state = (relative: string, key: string | null, changed = 0): RepositoryState => ({
  path: `/code/acme/${relative}`, relative, key, keys: key ? [key] : [], enclosing: false, branch: "main", upstream: "origin/main", ahead: 0, behind: 0, changed,
  readable: true, problem: null, error: null,
});
const repo = (id: string, repoKey: string) => ({
  id, provider: "github" as const, path: repoKey.split("/").slice(1).join("/"), repoKey, defaultBranch: null, private: null, description: null, webUrl: null,
});
const policy = (repositoryId: string | null): StoredPolicy => ({ ...emptyPolicy(), repositoryId, updatedAt: "" });

describe("chatRepositories", () => {
  it("joins the clones of the folder with the organization repositories and their policy", () => {
    const items = chatRepositories(
      [state("api", "github.com/acme/api", 2), state("scratch", null), state("web", "github.com/acme/web")],
      [repo("r1", "github.com/acme/api"), repo("r2", "github.com/acme/web"), repo("r3", "github.com/acme/worker")],
      [policy(null), policy("r1")],
    );
    expect(items.map((item) => [repositoryLabel(item), item.policy, item.repository?.id ?? null, item.state ? "in" : "out"])).toEqual([
      ["api", "repository", "r1", "in"],
      ["scratch", "organization", null, "in"],
      ["web", "organization", "r2", "in"],
      ["acme/worker", "organization", "r3", "out"],
    ]);
  });

  it("says there is no policy when neither the organization nor the repository has one", () => {
    const [item] = chatRepositories([state("api", "github.com/acme/api")], [repo("r1", "github.com/acme/api")], []);
    expect(item.policy).toBe("none");
  });

  it("finds the registered repository by any remote of the clone", () => {
    const fork = { ...state("api", "github.com/me/api"), keys: ["github.com/me/api", "github.com/acme/api"] };
    const items = chatRepositories([fork], [repo("r1", "github.com/acme/api")], []);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ key: "github.com/acme/api", repository: { id: "r1" } });
  });

  it("tells how to make git trust a folder of another owner", () => {
    expect(safeDirectoryCommand("D:\\code\\acme api")).toBe('git config --global --add safe.directory "D:/code/acme api"');
  });

  it("works offline, with only what git says", () => {
    const items = chatRepositories([state("", "github.com/acme/api")], [], []);
    expect(items).toHaveLength(1);
    expect(repositoryLabel(items[0])).toBe("github.com/acme/api");
  });
});
