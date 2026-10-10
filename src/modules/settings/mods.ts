import type { Key } from "@/modules/i18n";

/** A linha de comando de um mod criado em argumentos, como o shell separaria
 * com aspas simples e duplas — sem expandir nada. A mesma regra do
 * `split_line` do núcleo. */
export function splitLine(line: string): string[] {
  const args: string[] = [];
  let current = "";
  let quote: string | null = null;
  let started = false;
  for (const char of line) {
    if (quote) {
      if (char === quote) quote = null;
      else current += char;
    } else if (char === "\"" || char === "'") {
      quote = char;
      started = true;
    } else if (/\s/.test(char)) {
      if (started) { args.push(current); current = ""; started = false; }
    } else {
      current += char;
      started = true;
    }
  }
  if (started) args.push(current);
  return args;
}

/** Os argumentos de volta numa linha para editar: o que tem espaço ou aspas
 * vai entre aspas. */
export function joinLine(args: string[]): string {
  return args.map((arg) => {
    if (arg !== "" && !/[\s"']/.test(arg)) return arg;
    return arg.includes("\"") ? `'${arg}'` : `"${arg}"`;
  }).join(" ");
}

/** O lugar `{nome}` que a linha pode ter: `{model}` em qualquer parte e
 * `{prompt}` sozinho num argumento. Os outros são da montagem dos mods do app. */
const PLACEHOLDER = /\{[A-Za-z0-9_]+\}/g;

/** O que está errado numa linha de comando, ou `null`. A mesma conferência do
 * núcleo (`mods::custom`), mostrada antes de salvar. */
export function lineProblem(args: string[]): Key | null {
  if (args.length > 64) return "mods.args.tooMany";
  for (const arg of args) {
    if (arg === "" || arg.length > 1000 || /[\u0000-\u001f\u007f]/.test(arg)) return "mods.args.invalid";
    const rest = arg === "{prompt}" ? "" : arg.replaceAll("{model}", "");
    if ((rest.match(PLACEHOLDER) ?? []).length > 0) return "mods.args.placeholder";
  }
  return null;
}

/** O id de um mod criado a partir do nome: `mod-` e até 32 letras minúsculas,
 * dígitos ou `-`, sem repetir um id que já existe. */
export function modId(name: string, taken: readonly string[]): `mod-${string}` {
  const base = name.normalize("NFKD").replace(/[̀-ͯ]/g, "").toLowerCase()
    .replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 28).replace(/-+$/, "") || "mod";
  let id: `mod-${string}` = `mod-${base}`;
  for (let n = 2; taken.includes(id); n++) id = `mod-${base}-${n}`;
  return id;
}
