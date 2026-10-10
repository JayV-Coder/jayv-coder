/**
 * O que o build de produção tira da janela: tudo que é da webview (Chromium,
 * WebKit) e não do app — o menu de contexto, o inspetor e o código-fonte,
 * recarregar, imprimir, salvar a página, a busca nativa, o zoom, voltar e
 * avançar, abrir arquivo, favoritos, histórico, abas e janelas. O que o app
 * escuta continua valendo (Ctrl/⌘ + K, 1, 3, 4, vírgula, N, G, ponto, F), e a
 * edição de texto nos campos também (copiar, recortar, colar, selecionar tudo,
 * desfazer e refazer). Em desenvolvimento nada muda.
 */
import { shortcutFor } from "@/modules/commands";

/** O mínimo de um evento de teclado que decide o bloqueio. */
export type KeyPress = Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey">;

/** As teclas de função e de navegador que não fazem nada no app: ajuda (F1),
 * próxima ocorrência (F3), recarregar (F5), barra de endereço (F6), cursor de
 * navegação (F7), menu (F10), tela cheia (F11) e inspetor (F12). */
const BLOCKED_KEYS = new Set([
  "f1", "f3", "f5", "f6", "f7", "f10", "f11", "f12",
  "browserback", "browserforward", "browserrefresh", "browserstop", "browsersearch", "browserhome", "browserfavorites", "zoomin", "zoomout",
]);

/** Com Ctrl/⌘: recarregar, código-fonte, salvar, imprimir, buscar e achar
 * próxima, abrir arquivo, histórico, downloads, favorito, barra de endereço,
 * abas e janelas, e o zoom (+, -, 0). */
const BLOCKED_COMMANDS = new Set(["r", "u", "s", "p", "f", "g", "o", "h", "j", "d", "l", "e", "t", "w", "n", "q", "+", "=", "-", "_", "0", "tab", "pageup", "pagedown", "add", "subtract"]);

/** Com Ctrl/⌘ e Shift ou Alt: inspetor (I, J, C), recarregar sem cache (R),
 * favoritos (O, B), janela anônima e aba reaberta (N, T, W), limpar dados
 * (Delete), e de novo as de cima. */
const BLOCKED_EXTENDED = new Set(["i", "j", "c", "r", "o", "b", "n", "t", "w", "delete", "p", "s", "f", "g", "u", "d", "h", "e", "l"]);

/** Atalhos da webview que o app não usa. Os que o app escuta passam por
 * `isBlockedShortcut` como bloqueados também: quem chama decide se deixa o app
 * ouvir (`lockDownWebview` deixa, e cancela só o efeito nativo). */
export function isBlockedShortcut(press: KeyPress): boolean {
  const key = press.key.toLowerCase();
  const command = press.ctrlKey || press.metaKey;
  if (BLOCKED_KEYS.has(key)) return true;
  // Voltar, avançar e início do navegador: Alt + ←, → e Home.
  if (press.altKey && !command && (key === "arrowleft" || key === "arrowright" || key === "home")) return true;
  if (!command) return false;
  if ((press.shiftKey || press.altKey) && BLOCKED_EXTENDED.has(key)) return true;
  return BLOCKED_COMMANDS.has(key);
}

/** Um alvo onde o menu de contexto continua: campos de texto (copiar, colar). */
export function keepsContextMenu(target: EventTarget | null): boolean {
  if (typeof Element === "undefined" || !(target instanceof Element)) return false;
  return target.closest("input, textarea, [contenteditable=''], [contenteditable='true']") !== null;
}

/** Soltar um arquivo sobre a janela faria a webview abri-lo no lugar do app:
 * só os campos de texto aceitam soltar algo. */
function acceptsDrop(target: EventTarget | null): boolean {
  return keepsContextMenu(target);
}

/** Liga o bloqueio na janela, só no build de produção. */
export function lockDownWebview(production = import.meta.env.PROD): void {
  if (!production) return;
  window.addEventListener("contextmenu", (event) => {
    if (!keepsContextMenu(event.target)) event.preventDefault();
  }, { capture: true });
  window.addEventListener("keydown", (event) => {
    if (!isBlockedShortcut(event)) return;
    // O efeito nativo sempre sai. O app ainda ouve o que é dele (Ctrl+F abre a
    // busca da conversa); o resto não chega nem aos ouvintes da página.
    event.preventDefault();
    if (shortcutFor(event) === null) event.stopPropagation();
  }, { capture: true });
  // Ctrl + roda do mouse e o beliscar do touchpad dão zoom na webview.
  window.addEventListener("wheel", (event) => {
    if (event.ctrlKey || event.metaKey) event.preventDefault();
  }, { capture: true, passive: false });
  for (const name of ["gesturestart", "gesturechange", "gestureend"]) window.addEventListener(name, (event) => event.preventDefault(), { capture: true });
  for (const name of ["dragover", "drop"]) {
    window.addEventListener(name, (event) => { if (!acceptsDrop(event.target)) event.preventDefault(); }, { capture: true });
  }
}
