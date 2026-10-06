import { create } from "zustand";

/** A busca dentro da conversa aberta (Ctrl+F / ⌘F): se a barra está aberta e o
 * que foi digitado. `focus` sobe a cada pedido para abrir, para devolver o
 * foco ao campo quando a barra já está aberta. */
interface FindState {
  open: boolean;
  query: string;
  focus: number;
}

export const useChatFind = create<FindState>(() => ({ open: false, query: "", focus: 0 }));

export function openChatFind() {
  useChatFind.setState((state) => ({ open: true, focus: state.focus + 1 }));
}

export function closeChatFind() {
  useChatFind.setState({ open: false });
}

export function setFindQuery(query: string) {
  useChatFind.setState({ query });
}

/** A letra como a busca a lê: sem acento e sem diferença de caixa. */
function fold(char: string): string {
  return char.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** Onde `query` aparece em `text`, sem diferença de caixa nem de acento: o
 * começo e o fim (em unidades do texto original) de cada ocorrência, sem
 * sobreposição. */
export function matchSpans(text: string, query: string): [number, number][] {
  const needle = Array.from(query.trim()).map(fold).join("");
  if (!needle) return [];
  let folded = "";
  const origin: number[] = [];
  const length: number[] = [];
  for (let at = 0; at < text.length;) {
    const point = text.codePointAt(at) ?? 0;
    const char = String.fromCodePoint(point);
    const piece = fold(char);
    for (let index = 0; index < piece.length; index++) { origin.push(at); length.push(char.length); }
    folded += piece;
    at += char.length;
  }
  const spans: [number, number][] = [];
  for (let from = folded.indexOf(needle); from >= 0; from = folded.indexOf(needle, from + needle.length)) {
    const last = from + needle.length - 1;
    spans.push([origin[from], origin[last] + length[last]]);
  }
  return spans;
}

/** O índice que vem depois (ou antes) do atual, dando a volta nas pontas. */
export function stepIndex(current: number, total: number, direction: 1 | -1): number {
  if (total <= 0) return 0;
  return (current + direction + total) % total;
}

const SKIPPED = "script, style, textarea, input, [data-find-skip]";

/** Todas as ocorrências de `query` no texto visível de `root`, na ordem da
 * tela. Uma ocorrência que atravessa a borda de um elemento (negrito no meio
 * da palavra) não é achada. */
export function findRanges(root: HTMLElement, query: string): Range[] {
  if (!query.trim()) return [];
  const ranges: Range[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => (node.parentElement?.closest(SKIPPED) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
  });
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    for (const [start, end] of matchSpans(node.textContent ?? "", query)) {
      const range = document.createRange();
      range.setStart(node, start);
      range.setEnd(node, end);
      ranges.push(range);
    }
  }
  return ranges;
}

interface HighlightRegistry { set(name: string, value: unknown): void; delete(name: string): void }

const registry = (): HighlightRegistry | null => (typeof CSS !== "undefined" ? (CSS as unknown as { highlights?: HighlightRegistry }).highlights ?? null : null);
const HIGHLIGHT = "chat-find";
const CURRENT = "chat-find-current";

/** Pinta todas as ocorrências e a atual por cima, sem mexer no DOM da conversa
 * (que o React desenha). Onde a webview não tem a API de realce, a atual vira
 * a seleção do texto. */
export function paintMatches(ranges: Range[], current: number, scroll = true) {
  const highlights = registry();
  const Highlight = (globalThis as unknown as { Highlight?: new (...ranges: Range[]) => unknown }).Highlight;
  const active = ranges[current];
  if (highlights && Highlight) {
    highlights.set(HIGHLIGHT, new Highlight(...ranges.filter((_, index) => index !== current)));
    if (active) highlights.set(CURRENT, new Highlight(active)); else highlights.delete(CURRENT);
  } else if (active) {
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(active);
  }
  if (scroll) active?.startContainer.parentElement?.scrollIntoView({ block: "center", behavior: "auto" });
}

export function clearMatches() {
  const highlights = registry();
  highlights?.delete(HIGHLIGHT);
  highlights?.delete(CURRENT);
}
