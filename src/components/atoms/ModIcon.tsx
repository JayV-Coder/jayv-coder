import type { CSSProperties } from "react";
import { PuzzleIcon } from "lucide-react";
import { cn } from "@/lib/utils";

/** O SVG do ícone como endereço `data:`, para a imagem e a máscara. */
const uri = (svg: string) => `url("data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}")`;

/** O ícone de um mod, como o catálogo o traz. Nunca entra na página como
 * marcação: o desenho monocromático (`currentColor`) vira máscara pintada com a
 * cor do texto, e segue o tema; o colorido vira imagem, com as cores dele. Sem
 * ícone (ou com um que não é SVG), a peça de quebra-cabeça dos mods. */
export function ModIcon({ icon, className, title }: { icon?: string | null; className?: string; title?: string }) {
  const svg = icon?.trim();
  if (!svg || !svg.startsWith("<svg") || !svg.endsWith("</svg>")) return <PuzzleIcon aria-hidden="true" className={cn("shrink-0", className)} />;
  if (/currentColor/i.test(svg)) {
    const mask = uri(svg);
    const style: CSSProperties = {
      maskImage: mask, WebkitMaskImage: mask, maskSize: "contain", WebkitMaskSize: "contain",
      maskRepeat: "no-repeat", WebkitMaskRepeat: "no-repeat", maskPosition: "center", WebkitMaskPosition: "center",
    };
    return <span role={title ? "img" : undefined} aria-label={title} aria-hidden={title ? undefined : true} data-mod-icon="mask" className={cn("inline-block shrink-0 bg-current", className)} style={style} />;
  }
  return <img src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`} alt={title ?? ""} aria-hidden={title ? undefined : true} data-mod-icon="image" draggable={false} className={cn("shrink-0 object-contain", className)} />;
}
