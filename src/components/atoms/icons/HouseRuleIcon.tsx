import type { SVGProps } from "react";

/** A regra da casa: o que config.yaml permite. */
export function HouseRuleIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...props}>
      <path d="M2.8 8.8 10 2.9l7.2 5.9v8.3H2.8Z" />
      <path d="M5.9 12.6h8.2" />
    </svg>
  );
}
