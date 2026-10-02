import type { SVGProps } from "react";
import codex from "@/assets/codex.png";

/** O ícone oficial do Codex: a flor de seis pétalas no quadro claro. */
export function CodexIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" aria-hidden="true" {...props}>
      <image href={codex} width="24" height="24" />
    </svg>
  );
}
