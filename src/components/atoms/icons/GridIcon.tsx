import type { SVGProps } from "react";

/** Quatro pátios lado a lado: a grade da lista de projetos. */
export function GridIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" aria-hidden="true" {...props}>
      <g stroke="currentColor" strokeWidth="1.4">
        <rect x="1.7" y="1.7" width="5" height="5" />
        <rect x="9.3" y="1.7" width="5" height="5" />
        <rect x="1.7" y="9.3" width="5" height="5" />
        <rect x="9.3" y="9.3" width="5" height="5" />
      </g>
    </svg>
  );
}
