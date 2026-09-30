import type { SVGProps } from "react";

/** A flor de seis pétalas entrelaçadas do Codex. */
export function CodexIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" aria-hidden="true" {...props}>
      <g stroke="currentColor" strokeWidth="1.5">
        {Array.from({ length: 6 }, (_, petal) => (
          <rect key={petal} x="9.2" y="2.4" width="5.6" height="11" rx="2.8" transform={`rotate(${petal * 60} 12 12)`} />
        ))}
      </g>
    </svg>
  );
}
