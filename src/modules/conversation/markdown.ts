/** O Markdown que os modelos costumam escrever, lido em blocos e trechos. A
 * tela monta cada bloco como elemento — nada aqui vira HTML cru. */

export type Inline =
  | { type: "text"; text: string }
  | { type: "strong"; text: string }
  | { type: "code"; text: string }
  | { type: "link"; text: string; href: string };

export type Block =
  | { type: "heading"; level: number; inline: Inline[] }
  | { type: "paragraph"; inline: Inline[] }
  | { type: "list"; ordered: boolean; items: Inline[][] }
  | { type: "quote"; inline: Inline[] }
  | { type: "rule" }
  | { type: "table"; head: Inline[][]; rows: Inline[][][] }
  | { type: "code"; language: string; code: string };

export function parseInline(text: string): Inline[] {
  const pattern = /(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\(https?:\/\/[^)\s]+\))/g;
  const out: Inline[] = [];
  let cursor = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(text)) !== null) {
    if (match.index > cursor) out.push({ type: "text", text: text.slice(cursor, match.index) });
    const token = match[0];
    if (token.startsWith("**")) out.push({ type: "strong", text: token.slice(2, -2) });
    else if (token.startsWith("`")) out.push({ type: "code", text: token.slice(1, -1) });
    else {
      const link = token.match(/^\[([^\]]+)\]\((https?:\/\/[^)]+)\)$/);
      if (link) out.push({ type: "link", text: link[1], href: link[2] });
    }
    cursor = pattern.lastIndex;
  }
  if (cursor < text.length) out.push({ type: "text", text: text.slice(cursor) });
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

export function parseMarkdown(content: string): Block[] {
  const blocks: Block[] = [];
  const fence = /```([^\n`]*)\n?([\s\S]*?)```/g;
  let cursor = 0;
  let match: RegExpExecArray | null;
  while ((match = fence.exec(content)) !== null) {
    parseText(content.slice(cursor, match.index), blocks);
    blocks.push({ type: "code", language: match[1].trim() || "código", code: match[2].replace(/\n$/, "") });
    cursor = fence.lastIndex;
  }
  parseText(content.slice(cursor), blocks);
  return blocks;
}
