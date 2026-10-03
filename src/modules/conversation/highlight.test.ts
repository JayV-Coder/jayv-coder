import { describe, expect, it } from "vitest";
import { highlight } from "./highlight";

const kinds = (code: string, language: string) => highlight(code, language).map((line) => line.filter((token) => token.text.trim()).map((token) => `${token.kind}:${token.text.trim()}`));

describe("highlight", () => {
  it("pinta palavra reservada, texto, número, função e comentário", () => {
    expect(kinds('const name = "jayv"; // nome\nrun(42)', "ts")).toEqual([
      ["keyword:const", "plain:name =", "string:\"jayv\"", "plain:;", "comment:// nome"],
      ["fn:run", "plain:(", "number:42", "plain:)"],
    ]);
  });

  it("comentário com # no shell e com -- no SQL", () => {
    expect(kinds("# instala\nnpm ci", "bash")[0]).toEqual(["comment:# instala"]);
    expect(kinds("select 1 -- um", "sql")[0]).toEqual(["keyword:select", "number:1", "comment:-- um"]);
  });

  it("o comentário de várias linhas fica colorido em cada uma", () => {
    expect(kinds("/* a\nb */ x", "rs")).toEqual([["comment:/* a"], ["comment:b */", "plain:x"]]);
  });

  it("diff marca o que entrou e o que saiu", () => {
    expect(highlight("@@ -1 +1 @@\n-velho\n+novo\n igual", "diff").map((line) => line[0].kind)).toEqual(["hunk", "del", "add", "plain"]);
  });

  it("texto puro fica sem cor", () => {
    expect(kinds("if 1", "text")).toEqual([["plain:if 1"]]);
  });
});
