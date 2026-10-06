import { describe, expect, it } from "vitest";
import { parseInline, parseMarkdown } from "./markdown";

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

describe("caminhos e código que os modelos escrevem", () => {
  const file = (text: string) => parseInline(text).find((item) => item.type === "file");

  it("caminho absoluto com espaço e arquivo de ponto com extensão", () => {
    const path = "/home/dev/Meus Projetos/app/jobs/.env.example";
    expect(parseInline(`\`${path}\``)).toEqual([{ type: "file", text: path, path }]);
    expect(parseInline("`.env.example`")).toEqual([{ type: "file", text: ".env.example", path: ".env.example" }]);
  });

  it("caminhos do Windows, com disco, barras invertidas e espaço", () => {
    for (const path of ["C:\\Users\\dev\\Meus Projetos\\app\\main.rs", "src\\modules\\util.ts", "D:\\work\\Dockerfile"]) {
      expect(parseInline(`\`${path}\``)).toEqual([{ type: "file", text: path, path }]);
    }
    expect(file("Editei C:\\work\\app\\main.rs agora")).toEqual({ type: "file", text: "C:\\work\\app\\main.rs", path: "C:\\work\\app\\main.rs" });
  });

  it("posição no arquivo: linha, coluna e intervalo", () => {
    expect(file("`app.py:30`")).toEqual({ type: "file", text: "app.py:30", path: "app.py" });
    expect(file("`src/a.ts:10-24`")).toEqual({ type: "file", text: "src/a.ts:10-24", path: "src/a.ts" });
    expect(file("`src/a.ts:10:5`")).toEqual({ type: "file", text: "src/a.ts:10:5", path: "src/a.ts" });
    expect(file("`src/a.ts#L10-L20`")).toEqual({ type: "file", text: "src/a.ts#L10-L20", path: "src/a.ts" });
    expect(file("o erro está em browser_pool.py:30, veja")).toEqual({ type: "file", text: "browser_pool.py:30", path: "browser_pool.py" });
  });

  it("link com destino de arquivo vira selo, sem os colchetes", () => {
    expect(parseInline("[main](src/main.rs:12)")).toEqual([{ type: "file", text: "src/main.rs:12", path: "src/main.rs" }]);
    expect(parseInline("[ajuda](#secao)")).toEqual([{ type: "text", text: "ajuda" }]);
  });

  it("frase e comando em crase não viram arquivo", () => {
    for (const text of ["`rodar o teste, depois o build`", "`npm run build`", "`a / b`", "`std::fs::read`", "`v1.2.3`"]) {
      expect(file(text)).toBeUndefined();
    }
  });

  it("crase dupla, itálico e negrito", () => {
    expect(parseInline("use ``a `b` c`` aqui")[1]).toEqual({ type: "code", text: "a `b` c" });
    expect(parseInline("é *importante* mesmo")[1]).toEqual({ type: "em", text: "importante" });
    expect(parseInline("2 * 3 * 4")).toEqual([{ type: "text", text: "2 * 3 * 4" }]);
  });
});

describe("parseMarkdown", () => {
  it("bloco com cerca, língua e título", () => {
    expect(parseMarkdown("antes\n\n```ts title=\"a.ts\"\nconst a = 1;\n```\n\ndepois")).toEqual([
      { type: "paragraph", inline: [{ type: "text", text: "antes" }] },
      { type: "code", language: "ts", code: "const a = 1;" },
      { type: "paragraph", inline: [{ type: "text", text: "depois" }] },
    ]);
  });

  it("cerca sem fechar (resposta ainda chegando ou cortada) vai até o fim", () => {
    const blocks = parseMarkdown("veja:\n```rust\nfn main() {\n    run();");
    expect(blocks[1]).toEqual({ type: "code", language: "rust", code: "fn main() {\n    run();" });
  });

  it("cerca recuada, de til ou de quatro crases", () => {
    expect(parseMarkdown("1. passo\n   ```sh\n   npm test\n   ```")[1]).toEqual({ type: "code", language: "sh", code: "npm test" });
    expect(parseMarkdown("~~~py\nprint(1)\n~~~")[0]).toEqual({ type: "code", language: "py", code: "print(1)" });
    expect(parseMarkdown("````md\n```ts\nx\n```\n````")[0]).toEqual({ type: "code", language: "md", code: "```ts\nx\n```" });
  });

  it("três crases no meio da linha não abrem bloco", () => {
    expect(parseMarkdown("escreva ``` para abrir")[0].type).toBe("paragraph");
  });

  it("código recuado depois de linha em branco, sem cerca", () => {
    expect(parseMarkdown("Rode:\n\n    cargo test\n    cargo build\n\nPronto.")[1]).toEqual({ type: "code", language: "", code: "cargo test\ncargo build" });
  });

  it("sub-item de lista recuado continua lista", () => {
    const blocks = parseMarkdown("- um\n    - dois\n- três");
    expect(blocks).toHaveLength(1);
    expect(blocks[0].type).toBe("list");
  });
});
