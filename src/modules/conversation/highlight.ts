/** As cores do código da resposta, sem biblioteca: um leitor só, que conhece
 * o que quase toda linguagem tem em comum — comentário, texto entre aspas,
 * número, palavra reservada, tipo e chamada de função. Não é um compilador;
 * errar uma cor aqui custa pouco, e a leitura cansa bem menos do que um bloco
 * inteiro da mesma cor. */

export type TokenKind = "plain" | "comment" | "string" | "number" | "keyword" | "type" | "fn" | "add" | "del" | "hunk";

export interface Token {
  kind: TokenKind;
  text: string;
}

const KEYWORDS = new Set([
  // JS / TS
  "const", "let", "var", "function", "return", "if", "else", "for", "while", "do", "switch", "case", "default", "break",
  "continue", "import", "export", "from", "as", "class", "extends", "implements", "new", "this", "super", "try", "catch",
  "finally", "throw", "async", "await", "yield", "typeof", "instanceof", "in", "of", "void", "delete", "interface", "type",
  "enum", "readonly", "public", "private", "protected", "static", "abstract", "declare", "namespace", "keyof", "satisfies",
  // Rust
  "fn", "pub", "mod", "use", "struct", "impl", "trait", "match", "mut", "ref", "self", "Self", "crate", "where", "loop",
  "move", "dyn", "unsafe", "extern",
  // Python / Ruby / Go / outros
  "def", "elif", "lambda", "pass", "with", "global", "nonlocal", "raise", "except", "is", "not", "and", "or", "end",
  "func", "package", "go", "defer", "chan", "select", "range", "struct", "map",
  // valores
  "true", "false", "null", "undefined", "None", "True", "False", "nil",
]);

const SQL = new Set([
  "select", "from", "where", "insert", "into", "values", "update", "set", "delete", "create", "table", "alter", "drop",
  "add", "column", "index", "on", "join", "left", "right", "inner", "outer", "group", "by", "order", "having", "limit",
  "and", "or", "not", "null", "is", "as", "primary", "key", "references", "default", "unique", "conflict", "do", "returning",
  "begin", "commit", "policy", "for", "using", "with", "check", "grant", "to", "function", "returns", "language", "true", "false",
]);

const HASH_COMMENT = new Set(["py", "python", "sh", "bash", "zsh", "shell", "console", "yaml", "yml", "toml", "rb", "ruby", "ini", "conf", "dockerfile", "make", "makefile", "r", "ps1", "powershell"]);
const DASH_COMMENT = new Set(["sql", "lua", "haskell", "hs"]);
const PLAIN = new Set(["", "text", "txt", "plain", "plaintext", "output", "log"]);

/** O padrão de cada coisa, na ordem em que vale: o comentário ganha do texto
 * entre aspas que estiver dentro dele, e vice-versa. */
function patternFor(language: string) {
  const comments = [String.raw`\/\*[\s\S]*?(?:\*\/|$)`];
  if (HASH_COMMENT.has(language)) comments.push(String.raw`#[^\n]*`);
  else if (DASH_COMMENT.has(language)) comments.push(String.raw`--[^\n]*`);
  else comments.push(String.raw`\/\/[^\n]*`);
  if (language === "json" || language === "jsonc") comments.splice(0);
  const strings = String.raw`"(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*'|\x60(?:\\.|[^\x60\\])*\x60`;
  const number = String.raw`\b(?:0x[\da-fA-F]+|\d[\d_]*(?:\.\d+)?(?:e[+-]?\d+)?)\b`;
  const word = String.raw`[A-Za-z_$][\w$]*`;
  const parts = [...comments, strings, number, word].map((part) => `(${part})`);
  return { pattern: new RegExp(parts.join("|"), "g"), groups: parts.length };
}

function words(code: string, language: string): Token[] {
  const tokens: Token[] = [];
  const sql = language === "sql" || language === "psql" || language === "pgsql";
  const { pattern, groups } = patternFor(language);
  let cursor = 0;
  let match: RegExpExecArray | null;
  const push = (kind: TokenKind, text: string) => {
    const last = tokens[tokens.length - 1];
    if (last && last.kind === kind) last.text += text;
    else tokens.push({ kind, text });
  };
  while ((match = pattern.exec(code)) !== null) {
    if (match.index > cursor) push("plain", code.slice(cursor, match.index));
    const text = match[0];
    // A última captura é a palavra; a penúltima, o número; antes, os textos e os comentários.
    const index = match.slice(1).findIndex((group) => group !== undefined);
    const isWord = index === groups - 1;
    const isNumber = index === groups - 2;
    const isString = index === groups - 3;
    if (isWord) {
      const next = code.slice(pattern.lastIndex).match(/^\s*\(/);
      if (sql ? SQL.has(text.toLowerCase()) : KEYWORDS.has(text)) push("keyword", text);
      else if (next) push("fn", text);
      else if (/^[A-Z][a-z]\w*$/.test(text)) push("type", text);
      else push("plain", text);
    } else if (isNumber) push("number", text);
    else if (isString) push("string", text);
    else push("comment", text);
    cursor = pattern.lastIndex;
  }
  if (cursor < code.length) push("plain", code.slice(cursor));
  return tokens;
}

/** O código em linhas, cada linha nos seus pedaços coloridos. O comentário
 * ou o texto que atravessa linhas é cortado em cada uma, com a mesma cor. */
export function highlight(code: string, language: string): Token[][] {
  const lang = language.trim().toLowerCase();
  if (lang === "diff" || lang === "patch") {
    return code.split("\n").map((line) => {
      const kind: TokenKind = line.startsWith("@@") ? "hunk"
        : line.startsWith("+") && !line.startsWith("+++") ? "add"
        : line.startsWith("-") && !line.startsWith("---") ? "del"
        : "plain";
      return [{ kind, text: line }];
    });
  }
  const tokens = PLAIN.has(lang) ? [{ kind: "plain" as const, text: code }] : words(code, lang);
  const lines: Token[][] = [[]];
  tokens.forEach((token) => {
    token.text.split("\n").forEach((piece, at) => {
      if (at > 0) lines.push([]);
      if (piece) lines[lines.length - 1].push({ kind: token.kind, text: piece });
    });
  });
  return lines;
}
