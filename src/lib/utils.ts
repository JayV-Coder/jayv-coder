import { clsx, type ClassValue } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

/** A escala de texto do design system (`text-h1` … `text-caption`) é tamanho
 * de fonte. Sem isto o `tailwind-merge` a lê como cor e apaga a cor do texto
 * vizinha: o botão `text-primary-foreground text-caption` ficava com a letra
 * da cor do fundo. */
const merge = extendTailwindMerge({
  extend: { classGroups: { "font-size": [{ text: ["h1", "h2", "h3", "h4", "body", "small", "caption"] }] } },
});

export function cn(...inputs: ClassValue[]) {
  return merge(clsx(inputs));
}
