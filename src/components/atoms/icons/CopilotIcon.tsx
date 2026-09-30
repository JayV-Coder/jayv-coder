import type { SVGProps } from "react";

/** O rosto de óculos do Copilot. */
export function CopilotIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" aria-hidden="true" {...props}>
      <path d="M3.5 13.5c0-2 .6-3.6 1.6-4.6M20.5 13.5c0-2-.6-3.6-1.6-4.6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M3.5 13.2v3.1c0 .7.4 1.3 1 1.6 2.3 1.2 4.8 1.8 7.5 1.8s5.2-.6 7.5-1.8c.6-.3 1-.9 1-1.6v-3.1" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
      <rect x="4.6" y="5" width="6.6" height="6.2" rx="3" stroke="currentColor" strokeWidth="1.5" />
      <rect x="12.8" y="5" width="6.6" height="6.2" rx="3" stroke="currentColor" strokeWidth="1.5" />
      <path d="M10 14.2v2M14 14.2v2" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}
