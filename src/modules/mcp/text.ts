/** O texto do `/mcp` e dos campos de lista, sem nada do núcleo: dá para
 * testar sem a janela. */

/** `/mcp` no começo da mensagem: o resto é a configuração ou a descrição. */
export function mcpCommand(value: string): string | null {
  const match = /^\/mcp(?:\s+([\s\S]*))?$/i.exec(value.trim());
  return match ? (match[1] ?? "").trim() : null;
}

export const lines = (value: string) => value.split("\n").map((line) => line.trim()).filter(Boolean);

/** `CHAVE=valor` (ou `Chave: valor`), um por linha, e de volta. */
export function pairs(value: string, separator: "=" | ":"): Record<string, string> {
  const result: Record<string, string> = {};
  for (const line of lines(value)) {
    const at = line.indexOf(separator);
    if (at > 0) result[line.slice(0, at).trim()] = line.slice(at + 1).trim();
  }
  return result;
}

export function pairText(map: Record<string, string>, separator: "=" | ":"): string {
  return Object.entries(map).map(([key, value]) => `${key}${separator === "=" ? "=" : ": "}${value}`).join("\n");
}
