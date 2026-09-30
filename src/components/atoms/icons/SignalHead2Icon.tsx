import type { SVGProps } from "react";

/** Semáforo de dois aspectos do portão de saída: aqui não existe "pergunte antes", só liberado ou segurado, e a cabeça é mais curta por isso. */
export function SignalHead2Icon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 52" fill="none" aria-hidden="true" {...props}>
      <path className="mast" d="M13 46.5v5M8.5 51.5h9" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <path className="housing" d="M3.2 14.5a9.8 9.8 0 0 1 19.6 0v28A4 4 0 0 1 18.8 46H7.2a4 4 0 0 1-4-3.5Z" />
      <g className="lenses">
        <circle className="lens lens--stop" cx="13" cy="15.5" r="6.9" />
        <circle className="lens lens--go" cx="13" cy="33" r="6.9" />
      </g>
      <g className="hoods" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round">
        <path d="M4.9 12.6a9.1 9.1 0 0 1 16.2 0" />
        <path d="M4.9 30.1a9.1 9.1 0 0 1 16.2 0" />
      </g>
      <g className="glints">
        <path d="M9.4 12.2a5 5 0 0 1 3-1.9" />
        <path d="M9.4 29.7a5 5 0 0 1 3-1.9" />
      </g>
    </svg>
  );
}
