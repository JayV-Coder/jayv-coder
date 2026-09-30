import type { SVGProps } from "react";

/** Portão de entrada: a cancela erguida deixando o pedido passar. */
export function GateInIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.45" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...props}>
      <path d="M13.5 4.5v12" />
      <path d="M10.5 17h6" />
      <path d="m13.5 11.5 4.6-5.2" />
      <path d="M1.8 10.6h6.4" />
      <path d="m5.6 7.8 2.8 2.8-2.8 2.8" />
    </svg>
  );
}
