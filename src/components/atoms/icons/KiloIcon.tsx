import type { SVGProps } from "react";

/** O Kilo Code: um colchete de código com a barra do prompt. */
export function KiloIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" aria-hidden="true" {...props}>
      <path d="M8.5 7 3.8 12l4.7 5M15.5 7l4.7 5-4.7 5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M13 6.5 11 17.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" opacity=".45" />
    </svg>
  );
}
