import type { SVGProps } from "react";

/** O balde do Bitbucket, em traço. */
export function BitbucketIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" aria-hidden="true" {...props}>
      <path d="M3 4h18l-2.7 16H5.7z" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
      <path d="M9.4 9h5.2l-.9 5.2h-3.4z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
    </svg>
  );
}
