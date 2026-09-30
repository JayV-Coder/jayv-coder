import type { SVGProps } from "react";

/** A faísca do Claude: raios de comprimentos alternados em volta de um centro. */
export function ClaudeIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" aria-hidden="true" {...props}>
      <g stroke="#D97757" strokeWidth="2.1" strokeLinecap="round">
        {Array.from({ length: 12 }, (_, ray) => (
          <line key={ray} x1="12" y1={ray % 2 ? 5.2 : 2.6} x2="12" y2="9.6" transform={`rotate(${ray * 30} 12 12)`} />
        ))}
      </g>
    </svg>
  );
}
