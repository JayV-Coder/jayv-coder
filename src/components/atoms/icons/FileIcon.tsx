import type { SVGProps } from "react";

/** Um arquivo que o modelo quis mexer. */
export function FileIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...props}>
      <path d="M4.4 2.6h7.2l4 4v10.8H4.4Z" />
      <path d="M11.6 2.6v4h4" />
      <path d="M6.8 11.4h6.4M6.8 14.2h4.2" />
    </svg>
  );
}
