import type { SVGProps } from "react";

/** A raposa do GitLab, em traço. */
export function GitlabIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" aria-hidden="true" {...props}>
      <path d="M12 20.5 3 13.9l1.9-9.4 2.7 7.4h8.8l2.7-7.4 1.9 9.4z" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
      <path d="M7.6 11.9 12 20.5l4.4-8.6" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
    </svg>
  );
}
