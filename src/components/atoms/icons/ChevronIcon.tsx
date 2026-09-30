import type { SVGProps } from "react";

/** Abre e fecha os critérios de um item. */
export function ChevronIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...props}>
      <path d="m5.4 7.8 4.6 4.6 4.6-4.6" />
    </svg>
  );
}
