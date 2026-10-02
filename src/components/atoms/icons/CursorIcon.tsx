import type { SVGProps } from "react";

/** O cubo em perspectiva do Cursor. */
export function CursorIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" aria-hidden="true" {...props}>
      <path d="M12 2.6 20.2 7.3v9.4L12 21.4l-8.2-4.7V7.3z" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
      <path d="M3.8 7.3 12 12l8.2-4.7M12 12v9.4" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
      <path d="M12 12 20.2 7.3 12 2.6z" fill="currentColor" opacity=".35" />
    </svg>
  );
}
