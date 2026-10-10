import { create } from "zustand";
import type { WorkMode } from "@/modules/core";

/** O que um atalho de teclado global faz. A tela decide como cumprir; aqui só
 * se reconhece a tecla. */
export type Shortcut = "palette" | "projects" | "stats" | "system" | "settings" | "newChat" | "gate" | "workMode" | "find";

interface KeyLike {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  isComposing?: boolean;
}

/** A tecla de comando do sistema: ⌘ no Mac, Ctrl no resto. */
export function isMac(platform = typeof navigator === "undefined" ? "" : navigator.platform) {
  return /mac|iphone|ipad/i.test(platform);
}

/** Como a tecla de comando aparece num atalho. */
export const MOD = isMac() ? "⌘" : "Ctrl";

const BY_KEY: Record<string, Shortcut> = {
  k: "palette",
  "1": "projects",
  "3": "stats",
  "4": "system",
  ",": "settings",
  n: "newChat",
  g: "gate",
  ".": "workMode",
  f: "find",
};

/** Os comandos de modo que a caixa de mensagem entende, com os apelidos em
 * português. */
const MODE_COMMANDS: Record<string, WorkMode> = {
  auto: "auto", plan: "plan", build: "build", planejar: "plan", planejamento: "plan", desenvolver: "build", desenvolvimento: "build",
};

/** `/plan`, `/build` ou `/auto` no começo do texto: o modo pedido e o que
 * sobrou depois do comando (vazio quando o texto era só o comando). Outro
 * texto, ou outra barra (`/why`), não é comando de modo. */
export function modeCommand(text: string): { mode: WorkMode; rest: string } | null {
  const match = /^\/(\p{L}+)(?:\s+([\s\S]*))?$/u.exec(text.trim());
  const mode = match ? MODE_COMMANDS[match[1].toLowerCase()] : undefined;
  return mode ? { mode, rest: (match?.[2] ?? "").trim() } : null;
}

/** O atalho que uma tecla pressionada pede, ou nada. Só vale com a tecla de
 * comando do sistema (⌘ no Mac, Ctrl no resto) e sem Alt nem Shift, para não
 * roubar o que o campo de texto faz com elas. */
export function shortcutFor(event: KeyLike, mac = isMac()): Shortcut | null {
  if (event.isComposing || event.altKey || event.shiftKey) return null;
  const mod = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  if (!mod) return null;
  return BY_KEY[event.key.toLowerCase()] ?? null;
}

/** Os atalhos como a tela os escreve, na ordem em que aparecem. */
export function shortcutLabel(shortcut: Shortcut) {
  const key = Object.entries(BY_KEY).find(([, value]) => value === shortcut)?.[0] ?? "";
  return [MOD, key.toUpperCase()];
}

/** O atalho numa palavra só: `⌘K` no Mac, `Ctrl+K` no resto. */
export function shortcutText(shortcut: Shortcut) {
  const [mod, key] = shortcutLabel(shortcut);
  return MOD === "⌘" ? `${mod}${key}` : `${mod}+${key}`;
}

interface PaletteState {
  open: boolean;
}

/** A paleta de comandos (⌘K): aberta ou não. */
export const usePalette = create<PaletteState>(() => ({ open: false }));

export function setPaletteOpen(open: boolean) {
  usePalette.setState({ open });
}

export function togglePalette() {
  usePalette.setState((state) => ({ open: !state.open }));
}

/** Busca difusa da paleta: as letras do que foi digitado aparecem no nome, na
 * mesma ordem, sem precisar estar coladas. Devolve as posições casadas (para
 * grifar) ou `null`. Começo de palavra e letras seguidas pesam mais. */
export function fuzzyMatch(query: string, text: string): { score: number; positions: number[] } | null {
  const needle = query.trim().toLowerCase();
  if (!needle) return { score: 0, positions: [] };
  const haystack = text.toLowerCase();
  const positions: number[] = [];
  let score = 0;
  let from = 0;
  for (const char of needle) {
    if (char === " ") continue;
    const at = haystack.indexOf(char, from);
    if (at < 0) return null;
    const previous = positions[positions.length - 1];
    if (previous !== undefined && at === previous + 1) score += 3;
    if (at === 0 || /[\s\-_/.·›]/.test(haystack[at - 1] ?? "")) score += 2;
    score -= Math.min(at - from, 5) * 0.1;
    positions.push(at);
    from = at + 1;
  }
  return { score, positions };
}
export * from "./intents";
