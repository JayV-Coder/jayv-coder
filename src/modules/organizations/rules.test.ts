import { describe, expect, it } from "vitest";
import { can, slugify, slugOk } from "./rules";

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
