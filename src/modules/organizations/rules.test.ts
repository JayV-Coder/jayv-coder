import { describe, expect, it } from "vitest";
import { can, parseRepoUrl, slugify, slugOk } from "./rules";

describe("parseRepoUrl", () => {
  it("cada forma de URL dá a mesma chave (igual ao núcleo)", () => {
    for (const url of ["git@github.com:Acme/API.git", "https://github.com/acme/api", "https://user@github.com/acme/api.git/", "ssh://git@github.com:22/Acme/Api.git", "git://github.com/acme/api", "github.com/acme/api"]) {
      expect(parseRepoUrl(url), url).toEqual({ provider: "github", path: "acme/api", key: "github.com/acme/api" });
    }
  });
  it("subgrupos do GitLab e o host do Bitbucket", () => {
    expect(parseRepoUrl("git@gitlab.com:grupo/sub/app.git")).toEqual({ provider: "gitlab", path: "grupo/sub/app", key: "gitlab.com/grupo/sub/app" });
    expect(parseRepoUrl("https://bitbucket.org/acme/web.git")?.provider).toBe("bitbucket");
  });
  it("recusa o resto", () => {
    for (const url of ["git@git.empresa.local:acme/api.git", "https://github.com/acme", "/home/dev/repo.git", "https://github.com/acme/ap i", ""]) {
      expect(parseRepoUrl(url), url).toBeNull();
    }
  });
});

describe("slug", () => {
  it("sugere a partir do nome", () => {
    expect(slugify("Ação & Cia Ltda.")).toBe("acao-cia-ltda");
    expect(slugify("  Acme  ")).toBe("acme");
  });
  it("confere a regra do banco", () => {
    for (const slug of ["acme", "a1b", "minha-org", "x".repeat(40)]) expect(slugOk(slug), slug).toBe(true);
    for (const slug of ["ab", "-acme", "acme-", "Acme", "a_b", "x".repeat(41)]) expect(slugOk(slug), slug).toBe(false);
  });
});

describe("can", () => {
  it("segue a tabela de papéis", () => {
    expect(can.manage("maintainer")).toBe(true);
    expect(can.manage("member")).toBe(false);
    expect(can.changeRoles("maintainer")).toBe(false);
    expect(can.remove("maintainer", "member")).toBe(true);
    expect(can.remove("maintainer", "maintainer")).toBe(false);
    expect(can.remove("owner", "owner")).toBe(true);
    expect(can.delete("maintainer")).toBe(false);
  });
});
