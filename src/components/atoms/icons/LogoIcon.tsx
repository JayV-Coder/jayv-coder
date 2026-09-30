import type { SVGProps } from "react";

/** Marca da Portaria: um poste de sinal com a cancela erguida. */
export function LogoIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" aria-hidden="true" {...props}>
      <circle className="logo-lamp" cx="7" cy="6.5" r="3.1" fill="currentColor" stroke="none" />
      <circle className="logo-halo" cx="7" cy="6.5" r="5.8" strokeWidth="1" opacity=".38" />
      <path d="M7 12.6V26" />
      <path d="M2.6 26h8.8" />
      <path d="M9 17.4 28.4 11" />
      <path d="m14.8 15.5 1.5 2.4" strokeWidth="1.3" />
      <path d="m20.8 13.5 1.5 2.4" strokeWidth="1.3" />
    </svg>
  );
}
