import { describe, expect, it } from "vitest";
import type { ProjectMemory } from "@/modules/core";
import { notesUsed, snippetParts } from "./index";

describe("memória do projeto", () => {
  it("separa o trecho achado do resto", () => {
    expect(snippetParts("…the \u0002invoice\u0003 totals and \u0002invoice\u0003")).toEqual([
      { text: "…the ", hit: false },
      { text: "invoice", hit: true },
      { text: " totals and ", hit: false },
      { text: "invoice", hit: true },
    ]);
    expect(snippetParts("nada marcado")).toEqual([{ text: "nada marcado", hit: false }]);
  });

  it("conta só as notas no teto, não as receitas", () => {
    const note = { id: "1", projectId: "p", title: "", trigger: "", covers: "", source: "manual", createdAt: "", updatedAt: "" };
    const memory: ProjectMemory = {
      notes: [{ ...note, kind: "note", body: "ação" }, { ...note, id: "2", kind: "recipe", body: "receita longa" }],
      repeated: [], notesLimit: 2200, recipeLimit: 2000,
    };
    expect(notesUsed(memory)).toBe(4);
  });
});
