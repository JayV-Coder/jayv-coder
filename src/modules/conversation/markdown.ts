/** O Markdown que os modelos costumam escrever, lido em blocos e trechos. A
 * tela monta cada bloco como elemento — nada aqui vira HTML cru. */

export type Inline =
  | { type: "text"; text: string }
  | { type: "strong"; text: string }
  | { type: "em"; text: string }
  | { type: "code"; text: string }
  | { type: "file"; text: string; path: string }
  | { type: "link"; text: string; href: string };

export type Block =
  | { type: "heading"; level: number; inline: Inline[] }
  | { type: "paragraph"; inline: Inline[] }
  | { type: "list"; ordered: boolean; items: Inline[][] }
  | { type: "quote"; inline: Inline[] }
  | { type: "rule" }
  | { type: "table"; head: Inline[][]; rows: Inline[][][] }
  | { type: "code"; language: string; code: string };

const FILE_EXTENSIONS = new Set([
  // código e configuração
  "rs", "js", "mjs", "cjs", "ts", "tsx", "jsx", "vue", "svelte", "css", "scss", "sass", "less", "html", "htm",
  "json", "jsonc", "yaml", "yml", "toml", "ini", "cfg", "conf", "xml", "env", "lock", "gradle", "properties",
  "py", "rb", "go", "java", "kt", "kts", "swift", "c", "h", "cpp", "hpp", "cc", "cs", "php", "lua", "dart", "scala",
  "ex", "exs", "sql", "prisma", "graphql", "gql", "proto", "sh", "bash", "zsh", "fish", "ps1", "bat", "pem", "key",
  "zig", "hs", "clj", "erl", "jl", "tf", "tfvars", "hcl", "mod", "sum", "astro", "ejs", "hbs", "j2", "ipynb", "cmd", "psm1", "vb", "m", "mm",
  // modelos de configuração (`.env.example`, `config.json.sample`)
  "example", "sample", "template", "dist", "tpl", "tmpl", "defaults", "local",
  // texto e documentos
  "md", "mdx", "rst", "txt", "log", "csv", "tsv", "pdf", "doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "ppt", "pptx", "odp",
  // imagens, áudio e vídeo
  "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "icns", "tif", "tiff", "avif", "heic",
  "mp3", "wav", "ogg", "flac", "m4a", "mp4", "mov", "webm", "mkv", "avi",
  // pacotes
  "zip", "tar", "gz", "tgz", "7z", "rar",
]);
const FILE_NAMES = new Set(["Dockerfile", "Makefile", "Procfile", "Gemfile", "Rakefile", "Justfile", "LICENSE", "README"]);

/** O sufixo de posição de um caminho: `:12`, `:12:4`, `:12-30`, `#L12` ou `#L12-L30`. */
const POSITION = /(?::\d+(?:-\d+)?){1,2}$|#L\d+(?:-L?\d+)?$/;

/** O caminho de arquivo que um trecho em `código` cita, sem a posição
 * (`:linha`, `:linha-linha`, `#L12`) do fim — ou nada, quando o trecho é um
 * símbolo, um valor ou um comando. Aceita as duas barras (Windows), a letra
 * do disco e espaço no meio do caminho quando ele tem pasta. */
export function filePath(text: string): string | null {
  const path = text.trim().replace(/^\.[\\/]/, "").replace(POSITION, "");
  if (!path || path.length > 260 || /[*?<>|"(){}`]|::|:\/\//.test(path) || /[\\/]$/.test(path)) return null;
  const folders = /[\\/]/.test(path);
  // Espaço só vale com pasta no caminho e sem cara de frase: `Meu Emissor/x.py`.
  if (/\s/.test(path) && (!folders || /\s{2,}|[,;]\s|\s[-=]\s/.test(path))) return null;
  const name = path.split(/[\\/]/).pop() ?? path;
  if (!name || /^\s|\s$/.test(name)) return null;
  if (FILE_NAMES.has(name)) return path;
  // `.env`, `.gitignore`, `.env.example`: arquivo de ponto, com ou sem extensão.
  if (/^\.[A-Za-z][\w-]*(?:\.[\w-]+)*$/.test(name)) return path;
  const dot = name.lastIndexOf(".");
  if (dot > 0 && FILE_EXTENSIONS.has(name.slice(dot + 1).toLowerCase())) return path;
  // Extensão que a lista não conhece vale quando o caminho tem pasta e o nome
  // é um nome de arquivo (`ops/deploy.nomad`), não um número de versão.
  return folders && /^[\w@+-][\w@+.-]*\.[A-Za-z][A-Za-z0-9]{0,9}$/.test(name) ? path : null;
}

/** Um caminho escrito solto no texto, sem crase: precisa de pelo menos uma
 * barra, para `v0.42.0` ou `README.md` numa frase não virarem arquivo (ou de
 * `:linha` depois do nome, como em `app.py:30`). Vale também o caminho do
 * Windows, com a letra do disco ou com barras invertidas. A pontuação do fim
 * da frase fica de fora. */
const BARE_PATH = new RegExp(
  [
    String.raw`(?<![\w@./~\\-])[A-Za-z]:\\(?:[\w@. -]+\\)*[\w@.-]+(?::\d+(?:-\d+)?)?`,
    String.raw`(?<![\w@./~\\:-])(?:\.{1,2}[\\/]|~\/|\/)?[\w@.-]+(?:[\\/][\w@.-]+)+(?::\d+(?:[:-]\d+)?)?`,
    String.raw`(?<![\w@./~\\:-])[\w@-][\w@.-]*\.[A-Za-z][A-Za-z0-9]{0,9}:\d+(?:[:-]\d+)?`,
  ].join("|"),
  "g",
);

function textWithPaths(text: string, out: Inline[]) {
  let cursor = 0;
  let match: RegExpExecArray | null;
  BARE_PATH.lastIndex = 0;
  while ((match = BARE_PATH.exec(text)) !== null) {
    const token = match[0].replace(/[.,;:]+$/, "");
    const path = filePath(token);
    if (!path) continue;
    if (match.index > cursor) out.push({ type: "text", text: text.slice(cursor, match.index) });
    out.push({ type: "file", text: token, path });
    cursor = match.index + token.length;
  }
  if (cursor < text.length) out.push({ type: "text", text: text.slice(cursor) });
}

const INLINE = new RegExp(
  [
    String.raw`\*\*[^*\n]+\*\*`,
    String.raw`(\x60{1,3})(?!\x60)[^\n]*?[^\x60\n]\1(?!\x60)`,
    String.raw`\[[^\]\n]+\]\([^)\s]+\)`,
    String.raw`(?<![\w*])\*(?![\s*])[^*\n]+?(?<![\s*])\*(?![\w*])`,
  ].join("|"),
  "g",
);

export function parseInline(text: string): Inline[] {
  const out: Inline[] = [];
  let cursor = 0;
  let match: RegExpExecArray | null;
  INLINE.lastIndex = 0;
  while ((match = INLINE.exec(text)) !== null) {
    if (match.index > cursor) textWithPaths(text.slice(cursor, match.index), out);
    const token = match[0];
    if (token.startsWith("**")) out.push({ type: "strong", text: token.slice(2, -2) });
    else if (token.startsWith("`")) {
      const fence = match[1].length;
      // `` com crase `dentro` ``: as bordas de espaço não fazem parte do código.
      const code = token.slice(fence, -fence).replace(/^ (.*) $/, "$1");
      const path = filePath(code);
      out.push(path ? { type: "file", text: code, path } : { type: "code", text: code });
    } else if (token.startsWith("[")) {
      const link = token.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
      if (link && /^https?:\/\//.test(link[2])) out.push({ type: "link", text: link[1], href: link[2] });
      else if (link) {
        // `[main.rs](src/main.rs:12)`: o destino que é arquivo vira selo; o
        // resto fica só com o texto, sem os colchetes e o endereço cru.
        const target = decodeURI(link[2].replace(/^file:\/\//, ""));
        const path = filePath(target);
        out.push(path ? { type: "file", text: target.replace(/^\.[\\/]/, ""), path } : { type: "text", text: link[1] });
      }
    } else out.push({ type: "em", text: token.slice(1, -1) });
    cursor = INLINE.lastIndex;
  }
  if (cursor < text.length) textWithPaths(text.slice(cursor), out);
  return out;
}

const cells = (line: string) => line.replace(/^\s*\||\|\s*$/g, "").split("|").map((cell) => parseInline(cell.trim()));

function parseText(value: string, blocks: Block[]) {
  const lines = value.replace(/^\n+|\n+$/g, "").split("\n");
  for (let index = 0; index < lines.length;) {
    const line = lines[index];
    if (!line.trim()) { index += 1; continue; }
    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) { blocks.push({ type: "heading", level: Math.min(heading[1].length + 2, 6), inline: parseInline(heading[2]) }); index += 1; continue; }
    if (line.includes("|") && lines[index + 1]?.match(/^\s*\|?\s*:?-+/)) {
      const head = cells(line);
      const rows: Inline[][][] = [];
      index += 2;
      while (index < lines.length && lines[index].includes("|")) { rows.push(cells(lines[index])); index += 1; }
      blocks.push({ type: "table", head, rows });
      continue;
    }
    const ordered = /^\s*\d+[.)]\s+(.+)$/.test(line);
    if (ordered || /^\s*[-*]\s+(.+)$/.test(line)) {
      const pattern = ordered ? /^\s*\d+[.)]\s+(.+)$/ : /^\s*[-*]\s+(.+)$/;
      const items: Inline[][] = [];
      while (index < lines.length) { const item = lines[index].match(pattern); if (!item) break; items.push(parseInline(item[1])); index += 1; }
      blocks.push({ type: "list", ordered, items });
      continue;
    }
    if (/^>\s?/.test(line)) {
      const parts: string[] = [];
      while (index < lines.length && /^>\s?/.test(lines[index])) { parts.push(lines[index].replace(/^>\s?/, "")); index += 1; }
      blocks.push({ type: "quote", inline: parseInline(parts.join("\n")) });
      continue;
    }
    if (/^\s*---+\s*$/.test(line)) { blocks.push({ type: "rule" }); index += 1; continue; }
    const paragraph: string[] = [];
    while (index < lines.length && lines[index].trim() && !/^(#{1,4})\s+/.test(lines[index]) && !/^\s*(?:[-*]|\d+[.)])\s+/.test(lines[index]) && !/^>\s?/.test(lines[index])) {
      paragraph.push(lines[index]);
      index += 1;
    }
    blocks.push({ type: "paragraph", inline: parseInline(paragraph.join("\n")) });
  }
}

const OPENING = /^(\s*)(`{3,}|~{3,})\s*([^\n`]*?)\s*$/;

/** O texto de um trecho sem cerca, com os blocos recuados (4 espaços ou tab,
 * depois de uma linha em branco) lidos como código. */
function parsePlain(value: string, blocks: Block[]) {
  const lines = value.split("\n");
  let prose: string[] = [];
  const flush = () => { if (prose.length) parseText(prose.join("\n"), blocks); prose = []; };
  for (let index = 0; index < lines.length;) {
    const indented = /^(?: {4}|\t)\S/.test(lines[index]);
    const after = index === 0 ? "" : lines[index - 1];
    // Sub-item de lista recuado não é código: só vale depois de linha em branco
    // e fora de lista.
    const listBefore = prose.some((line) => /^\s*(?:[-*+]|\d+[.)])\s+/.test(line)) && !/^\s*$/.test(prose[prose.length - 1] ?? "x");
    if (indented && !/^\s*(?:[-*+]|\d+[.)])\s/.test(lines[index]) && (index === 0 || !after.trim()) && !listBefore) {
      const code: string[] = [];
      while (index < lines.length && (/^(?: {4}|\t)/.test(lines[index]) || (!lines[index].trim() && /^(?: {4}|\t)/.test(lines[index + 1] ?? "")))) {
        code.push(lines[index].replace(/^(?: {4}|\t)/, ""));
        index += 1;
      }
      flush();
      blocks.push({ type: "code", language: "", code: code.join("\n").replace(/\n+$/, "") });
      continue;
    }
    prose.push(lines[index]);
    index += 1;
  }
  flush();
}

/** O Markdown em blocos. A cerca de código é de linha inteira (``` ou ~~~,
 * com recuo, de três ou mais), fecha com a mesma marca e, sem fechar — o
 * modelo ainda escrevendo, ou cortado —, vai até o fim da resposta. */
export function parseMarkdown(content: string): Block[] {
  const blocks: Block[] = [];
  const lines = content.replace(/\r\n?/g, "\n").split("\n");
  let text: string[] = [];
  const flush = () => { if (text.length) parsePlain(text.join("\n"), blocks); text = []; };
  for (let index = 0; index < lines.length;) {
    const open = lines[index].match(OPENING);
    if (!open) { text.push(lines[index]); index += 1; continue; }
    const [, indent, mark, info] = open;
    const closing = new RegExp(`^\\s*${mark[0] === "`" ? "`" : "~"}{${mark.length},}\\s*$`);
    const body: string[] = [];
    index += 1;
    while (index < lines.length && !closing.test(lines[index])) {
      body.push(lines[index].startsWith(indent) ? lines[index].slice(indent.length) : lines[index].trimStart());
      index += 1;
    }
    index += 1;
    flush();
    // `ts title="a.ts"` e `{.ts}`: a linguagem é a primeira palavra.
    const language = (info.split(/\s+/)[0] ?? "").replace(/^\{?\.?|\}$/g, "").toLowerCase();
    blocks.push({ type: "code", language, code: body.join("\n") });
  }
  flush();
  return blocks;
}
