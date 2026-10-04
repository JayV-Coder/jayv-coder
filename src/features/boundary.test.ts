import { readdirSync, readFileSync } from "node:fs";
import { join, relative, resolve, sep } from "node:path";
import { describe, expect, it } from "vitest";

/** A regra da fronteira: uma funcionalidade só entra em outra pelo
 * `index.ts` dela, e o que fica embaixo (`modules/`, `components/`, `lib/`)
 * não depende de funcionalidade nenhuma. */
const SRC = resolve(__dirname, "..");
const FEATURES = join(SRC, "features");

function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return files(path);
    return /\.tsx?$/.test(entry.name) ? [path] : [];
  });
}

function imports(file: string) {
  return [...readFileSync(file, "utf8").matchAll(/(?:from|import)\s*\(?\s*["']([^"']+)["']/g)].map((match) => match[1]);
}

/** A funcionalidade dona do arquivo (a pasta logo abaixo de `features/`), ou
 * nada quando o arquivo é da raiz de `features/`. */
function featureOf(file: string) {
  const parts = relative(FEATURES, file).split(sep);
  return parts.length > 1 ? parts[0] : null;
}

/** Para onde o import aponta, como caminho dentro de `src/`; os pacotes ficam de fora. */
function target(file: string, specifier: string) {
  if (specifier.startsWith("@/")) return specifier.slice(2);
  if (specifier.startsWith(".")) return relative(SRC, resolve(join(file, ".."), specifier)).split(sep).join("/");
  return null;
}

describe("fronteira das funcionalidades", () => {
  it("uma funcionalidade só importa outra pelo index dela", () => {
    const crossings = files(FEATURES).flatMap((file) => {
      const own = featureOf(file);
      return imports(file).flatMap((specifier) => {
        const path = target(file, specifier);
        const match = path?.match(/^features\/([^/]+)(\/.*)?$/);
        if (!match || match[1] === own) return [];
        const deep = match[2] !== undefined && match[2] !== "/index";
        return deep ? [`${relative(SRC, file)} → ${specifier}`] : [];
      });
    });
    expect(crossings).toEqual([]);
  });

  it("quem está fora de features/ entra só pelo index de cada uma", () => {
    const outside = files(SRC).filter((file) => !file.startsWith(FEATURES + sep));
    const deep = outside.flatMap((file) => imports(file)
      .filter((specifier) => /^features\/[^/]+\/./.test(target(file, specifier) ?? ""))
      .map((specifier) => `${relative(SRC, file)} → ${specifier}`));
    expect(deep).toEqual([]);
  });

  it("modules/, components/ e lib/ não dependem de funcionalidade", () => {
    const lower = ["modules", "components", "lib"].flatMap((dir) => files(join(SRC, dir)));
    const upward = lower.flatMap((file) => imports(file)
      .filter((specifier) => (target(file, specifier) ?? "").startsWith("features"))
      .map((specifier) => `${relative(SRC, file)} → ${specifier}`));
    expect(upward).toEqual([]);
  });

  it("o teste enxerga as funcionalidades", () => {
    expect(files(FEATURES).some((file) => featureOf(file) === "system")).toBe(true);
  });
});
