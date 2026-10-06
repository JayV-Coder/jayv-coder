import type { SVGProps } from "react";

/** O LiteLLM: um servidor no meio, ligado a vários modelos. */
export function LiteLLMIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" aria-hidden="true" {...props}>
      <rect x="9" y="9" width="6" height="6" rx="1.5" stroke="currentColor" strokeWidth="1.6" />
      <path d="M12 9V4.5M12 15v4.5M9 12H4.5M15 12h4.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <circle cx="12" cy="3.5" r="1.4" fill="currentColor" opacity=".45" /><circle cx="12" cy="20.5" r="1.4" fill="currentColor" opacity=".45" /><circle cx="3.5" cy="12" r="1.4" fill="currentColor" opacity=".45" /><circle cx="20.5" cy="12" r="1.4" fill="currentColor" opacity=".45" />
    </svg>
  );
}
