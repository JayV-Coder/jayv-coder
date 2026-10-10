/** Um pedaço do texto de um passo: as frases do resumo ou as do uso. */
export interface Segment { kind: "summary" | "usage"; text: string }

/** Uma parte do passo: o que cabe junto no cartão do tutorial. */
export type Page = Segment[];

/** Quanto texto cabe numa parte, em larguras de letra (um ideograma vale
 * dois). Passando disso, o passo se divide em partes de tamanho parecido. */
export const PAGE_BUDGET = 360;

/** Um texto um pouco maior que uma parte ainda fica numa só: cortar um pedaço
 * pequeno no fim atrapalha mais do que ajuda. */
const SLACK = 1.25;

// Ideogramas e sílabas (chinês, japonês, coreano) e as formas de largura cheia.
const WIDE = /[ᄀ-ᅟ⺀-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦]/u;

/** A largura de um texto: letra comum vale um, ideograma vale dois. */
export function textWidth(text: string) {
  let width = 0;
  for (const char of text) width += WIDE.test(char) ? 2 : 1;
  return width;
}

/** As frases de um texto, com o espaço que vem depois de cada uma (juntas,
 * refazem o texto). Usa o `Intl.Segmenter` do idioma; sem ele, corta depois
 * do ponto final, de exclamação ou de interrogação. */
export function sentences(text: string, locale: string): string[] {
  const clean = text.trim();
  if (!clean) return [];
  if (typeof Intl !== "undefined" && "Segmenter" in Intl) {
    try {
      return [...new Intl.Segmenter(locale, { granularity: "sentence" }).segment(clean)].map((part) => part.segment).filter((part) => part.trim());
    } catch {
      // Idioma que o Segmenter não conhece: vale o corte simples.
    }
  }
  return clean.match(/[^.!?。！？।؟]+(?:[.!?。！？।؟]+|$)\s*/gu) ?? [clean];
}

/** Junta as frases seguidas do mesmo tipo num pedaço só. */
function merge(parts: Segment[]): Page {
  const page: Page = [];
  for (const part of parts) {
    const last = page[page.length - 1];
    if (last?.kind === part.kind) last.text += part.text;
    else page.push({ ...part });
  }
  return page.map((part) => ({ ...part, text: part.text.trim() }));
}

/** Divide as larguras em `count` partes seguidas, com a maior parte o menor
 * possível (partição linear): devolve onde cada parte começa. */
function split(widths: number[], count: number): number[] {
  const n = widths.length;
  const prefix = [0];
  for (const width of widths) prefix.push(prefix[prefix.length - 1] + width);
  const sum = (from: number, to: number) => prefix[to] - prefix[from];
  // best[j][i]: a menor "maior parte" pondo as i primeiras frases em j partes.
  const best = Array.from({ length: count + 1 }, () => new Array<number>(n + 1).fill(Infinity));
  const cut = Array.from({ length: count + 1 }, () => new Array<number>(n + 1).fill(0));
  best[0][0] = 0;
  for (let j = 1; j <= count; j++) {
    for (let i = j; i <= n; i++) {
      for (let from = j - 1; from < i; from++) {
        const value = Math.max(best[j - 1][from], sum(from, i));
        if (value < best[j][i]) { best[j][i] = value; cut[j][i] = from; }
      }
    }
  }
  const starts: number[] = [];
  for (let j = count, i = n; j > 0; j--) { starts.unshift(cut[j][i]); i = cut[j][i]; }
  return starts;
}

/** Divide o texto de um passo (resumo e uso) em partes que cabem no cartão,
 * sem cortar frase: o passo curto fica numa parte só; o longo, no menor número
 * de partes em que nenhuma passa muito do tamanho de uma, com o texto
 * repartido por igual entre elas (nada de uma frase sobrando sozinha no fim). */
export function paginate(summary: string, usage: string, locale: string, budget = PAGE_BUDGET): Page[] {
  const parts: Segment[] = [
    ...sentences(summary, locale).map((text) => ({ kind: "summary" as const, text })),
    ...sentences(usage, locale).map((text) => ({ kind: "usage" as const, text })),
  ];
  if (!parts.length) return [];
  const widths = parts.map((part) => textWidth(part.text));
  const total = widths.reduce((sum, width) => sum + width, 0);
  if (total <= budget * SLACK) return [merge(parts)];
  let starts: number[] = [0];
  for (let count = Math.ceil(total / budget); count <= parts.length; count++) {
    starts = split(widths, count);
    const largest = Math.max(...starts.map((start, index) => widths.slice(start, starts[index + 1] ?? widths.length).reduce((sum, width) => sum + width, 0)));
    if (largest <= budget * SLACK) break;
  }
  return starts.map((start, index) => merge(parts.slice(start, starts[index + 1] ?? parts.length)));
}
