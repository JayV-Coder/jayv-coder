import { describe, expect, it } from "vitest";
import { parseInline } from "./markdown";

describe("parseInline", () => {
  it("acha o caminho escrito solto, sem a pontuação do fim", () => {
    expect(parseInline("Mudei src/app/main.tsx:12, e pronto.")).toEqual([
      { type: "text", text: "Mudei " },
      { type: "file", text: "src/app/main.tsx:12", path: "src/app/main.tsx" },
      { type: "text", text: ", e pronto." },
    ]);
  });

  it("não confunde versão, nome solto ou URL com arquivo", () => {
    const kinds = (text: string) => parseInline(text).map((item) => item.type);
    expect(kinds("Saiu a v0.42.0 e o README.md")).toEqual(["text"]);
    expect(kinds("veja https://example.com/a/b.js")).toEqual(["text"]);
    expect(kinds("ida/volta sem extensão")).toEqual(["text"]);
  });

  it("o caminho em crase continua sendo arquivo", () => {
    expect(parseInline("`./src/lib/utils.ts`")).toEqual([{ type: "file", text: "./src/lib/utils.ts", path: "src/lib/utils.ts" }]);
  });
});
