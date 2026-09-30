import type { SVGProps } from "react";

/** Um comando que o modelo pediu para rodar. */
export function CommandIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...props}>
      <path d="M2.6 3.8h14.8v12.4H2.6Z" />
      <path d="m6 8.4 2.3 2.3L6 13" />
      <path d="M10.6 13h3.4" />
    </svg>
  );
}
