/** Uma linha do diff: igual nos dois lados, só no antes, ou só no agora. Os
 * números são a linha em cada lado (nulo do lado em que ela não existe). */
export interface DiffLine { kind: "same" | "removed" | "added"; text: string; before: number | null; after: number | null }

/** Um trecho do diff: as linhas que mudaram com até `context` linhas iguais
 * em volta. `skipped` diz quantas linhas iguais ficaram de fora antes dele. */
export interface Hunk { skipped: number; lines: DiffLine[] }

const split = (text: string) => (text === "" ? [] : text.replace(/\r\n/g, "\n").replace(/\n$/, "").split("\n"));

/** O caminho mais curto de edição (Myers, O((N+M)·D)) entre as linhas do antes
 * e do agora. Diferença enorme demais (mais que `limit` passos) vira "tudo
 * saiu, tudo entrou", para não travar a tela num arquivo reescrito inteiro. */
export function lineDiff(before: string, after: string, limit = 4000): DiffLine[] {
  const a = split(before);
  const b = split(after);
  // As pontas iguais saem antes: é o caso comum, e encurta o meio.
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) { endA--; endB--; }
  const middle = myers(a.slice(start, endA), b.slice(start, endB), limit);
  const lines: DiffLine[] = [];
  for (let index = 0; index < start; index++) lines.push({ kind: "same", text: a[index], before: index + 1, after: index + 1 });
  let lineA = start;
  let lineB = start;
  for (const kind of middle) {
    if (kind === "same") { lines.push({ kind, text: a[lineA], before: lineA + 1, after: lineB + 1 }); lineA++; lineB++; }
    else if (kind === "removed") { lines.push({ kind, text: a[lineA], before: lineA + 1, after: null }); lineA++; }
    else { lines.push({ kind, text: b[lineB], before: null, after: lineB + 1 }); lineB++; }
  }
  for (let offset = 0; endA + offset < a.length; offset++) {
    lines.push({ kind: "same", text: a[endA + offset], before: endA + offset + 1, after: endB + offset + 1 });
  }
  return lines;
}

function myers(a: string[], b: string[], limit: number): DiffLine["kind"][] {
  const n = a.length;
  const m = b.length;
  if (n === 0) return Array<DiffLine["kind"]>(m).fill("added");
  if (m === 0) return Array<DiffLine["kind"]>(n).fill("removed");
  const max = Math.min(n + m, limit);
  const offset = max + 1;
  let v = new Int32Array(2 * max + 3);
  const trace: Int32Array[] = [];
  for (let d = 0; d <= max; d++) {
    trace.push(v.slice());
    const next = v.slice();
    for (let k = -d; k <= d; k += 2) {
      let x = k === -d || (k !== d && v[offset + k - 1] < v[offset + k + 1]) ? v[offset + k + 1] : v[offset + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && a[x] === b[y]) { x++; y++; }
      next[offset + k] = x;
      if (x >= n && y >= m) return backtrack(trace, d, offset, n, m);
    }
    v = next;
  }
  return [...Array<DiffLine["kind"]>(n).fill("removed"), ...Array<DiffLine["kind"]>(m).fill("added")];
}

function backtrack(trace: Int32Array[], depth: number, offset: number, n: number, m: number): DiffLine["kind"][] {
  const kinds: DiffLine["kind"][] = [];
  let x = n;
  let y = m;
  for (let d = depth; d > 0; d--) {
    const previous = trace[d];
    const k = x - y;
    const down = k === -d || (k !== d && previous[offset + k - 1] < previous[offset + k + 1]);
    const prevK = down ? k + 1 : k - 1;
    const prevX = previous[offset + prevK];
    const prevY = prevX - prevK;
    while (x > prevX && y > prevY) { kinds.push("same"); x--; y--; }
    kinds.push(down ? "added" : "removed");
    if (down) y--; else x--;
  }
  while (x > 0 && y > 0) { kinds.push("same"); x--; y--; }
  return kinds.reverse();
}

/** Quantas linhas entraram e saíram. */
export function diffCounts(lines: DiffLine[]) {
  let added = 0;
  let removed = 0;
  for (const line of lines) {
    if (line.kind === "added") added++;
    else if (line.kind === "removed") removed++;
  }
  return { added, removed };
}

/** Junta as linhas em trechos, como o `git diff`: cada mudança com `context`
 * linhas iguais em volta, e trechos que se encostam viram um só. */
export function hunks(lines: DiffLine[], context = 3): Hunk[] {
  const changed = lines.map((line, index) => (line.kind === "same" ? -1 : index)).filter((index) => index >= 0);
  if (changed.length === 0) return [];
  const ranges: [number, number][] = [];
  for (const index of changed) {
    const from = Math.max(0, index - context);
    const to = Math.min(lines.length - 1, index + context);
    const last = ranges[ranges.length - 1];
    if (last && from <= last[1] + 1) last[1] = Math.max(last[1], to);
    else ranges.push([from, to]);
  }
  let shown = 0;
  return ranges.map(([from, to]) => {
    const hunk = { skipped: from - shown, lines: lines.slice(from, to + 1) };
    shown = to + 1;
    return hunk;
  });
}

/** A primeira linha do agora que mudou: é nela que o editor abre. */
export function firstChangedLine(lines: DiffLine[]): number {
  const index = lines.findIndex((line) => line.kind !== "same");
  if (index < 0) return 1;
  for (let at = index; at < lines.length; at++) if (lines[at].after !== null) return lines[at].after!;
  for (let at = index; at >= 0; at--) if (lines[at].after !== null) return lines[at].after!;
  return 1;
}
