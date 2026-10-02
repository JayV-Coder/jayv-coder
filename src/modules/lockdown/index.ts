/**
 * O que o build de produção tira da janela: o menu de contexto da webview
 * (com "Inspecionar" e "Exibir código-fonte") e os atalhos que abrem o
 * inspetor, o código-fonte ou recarregam a página. Em desenvolvimento nada
 * muda.
 */

/** O mínimo de um evento de teclado que decide o bloqueio. */
export type KeyPress = Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey">;

/** Atalhos de inspetor, código-fonte, salvar página e recarregar. */
export function isBlockedShortcut(press: KeyPress): boolean {
  const key = press.key.toLowerCase();
  const command = press.ctrlKey || press.metaKey;
  if (key === "f12" || key === "f5" || key === "f7" || key === "browserrefresh") return true;
  if (command && (key === "r" || key === "u" || key === "s" || key === "p")) return true;
  // Ctrl+Shift+I/J/C no Windows e no Linux; Cmd+Option+I/J/C no macOS.
  if (command && (press.shiftKey || press.altKey) && (key === "i" || key === "j" || key === "c")) return true;
  return false;
}

/** Um alvo onde o menu de contexto continua: campos de texto (copiar, colar). */
export function keepsContextMenu(target: EventTarget | null): boolean {
  if (typeof Element === "undefined" || !(target instanceof Element)) return false;
  return target.closest("input, textarea, [contenteditable=''], [contenteditable='true']") !== null;
}

/** Liga o bloqueio na janela, só no build de produção. */
export function lockDownWebview(production = import.meta.env.PROD): void {
  if (!production) return;
  window.addEventListener("contextmenu", (event) => {
    if (!keepsContextMenu(event.target)) event.preventDefault();
  }, { capture: true });
  window.addEventListener("keydown", (event) => {
    if (isBlockedShortcut(event)) {
      event.preventDefault();
      event.stopPropagation();
    }
  }, { capture: true });
}
