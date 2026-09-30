import type { SVGProps } from "react";

/** Balão de conversa com a seta de voltar para ele: o botão de referência que a Portaria põe em cada pedido analisado. */
export function ChatRefIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" aria-hidden="true" {...props}>
      <path d="M14 9.4a1.6 1.6 0 0 1-1.6 1.6H6.6L3.2 13.8V11H3a1.6 1.6 0 0 1-1.6-1.6V4.2A1.6 1.6 0 0 1 3 2.6h9.4A1.6 1.6 0 0 1 14 4.2Z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
      <path d="M6.2 6.8h5M6.2 6.8l1.9-1.9M6.2 6.8l1.9 1.9" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
