import { isText, say, t, type Key, type Text } from "@/modules/i18n";

/** O prefixo dos avisos que o núcleo grava como mensagem (ver
 * `src-tauri/crates/jayv-base/src/i18n.rs`). */
const NOTICE = "jayv:notice:";

/** O aviso gravado no chat, no idioma de quem lê — ou nada, quando a mensagem
 * é texto comum. Itens de lista seguidos ficam na mesma lista. */
export function noticeText(content: string): string | null {
  if (!content.startsWith(NOTICE)) return null;
  let lines: unknown;
  try { lines = JSON.parse(content.slice(NOTICE.length)); } catch { return null; }
  if (!Array.isArray(lines) || !lines.every(isText)) return null;
  return (lines as Text[]).map(say).reduce((all, line) => {
    if (!all) return line;
    const item = line.startsWith("- ") && all.slice(all.lastIndexOf("\n") + 1).startsWith("- ");
    return `${all}${item ? "\n" : "\n\n"}${line}`;
  }, "");
}

/** As respostas a perguntas do agente, uma por pergunta, quando a mensagem é
 * só isso — ou nada. É o que o chat deixa recolher. */
export function answerLines(content: string): string[] | null {
  if (!content.startsWith(NOTICE)) return null;
  let lines: unknown;
  try { lines = JSON.parse(content.slice(NOTICE.length)); } catch { return null; }
  if (!Array.isArray(lines) || lines.length === 0 || !lines.every((line) => isText(line) && line.key === "ask.answer")) return null;
  return (lines as Text[]).map(say);
}

/** A mensagem como a tela a mostra. */
export function shownText(content: string): string {
  return noticeText(content) ?? content;
}

const SOURCES: Record<string, Key> = {
  jev: "source.jev",
  local: "source.local",
  heuristic: "source.local",
  heuristic_after_jev_error: "source.fallback",
  gate: "source.gate",
  permission: "source.permission",
  // A grafia dos registros gravados antes da troca para identificadores.
  "heurística local": "source.local",
};

/** De onde veio uma leitura (o Jev ou as heurísticas locais), no idioma de
 * quem lê. */
export function sourceLabel(source: string): string {
  const key = SOURCES[source];
  return key ? t(key) : source;
}
