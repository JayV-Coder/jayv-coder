import type { ReactNode, SVGProps } from "react";
import type { Locale } from "@/modules/i18n";
import { cn } from "@/lib/utils";

/** Os cinco pontos de uma estrela centrada em (x, y). */
function star(x: number, y: number, r: number) {
  return Array.from({ length: 10 }, (_, i) => {
    const radius = i % 2 === 0 ? r : r * 0.382;
    const angle = -Math.PI / 2 + (i * Math.PI) / 5;
    return `${(x + radius * Math.cos(angle)).toFixed(2)},${(y + radius * Math.sin(angle)).toFixed(2)}`;
  }).join(" ");
}

const bands = (colors: string[], vertical = false) => colors.map((fill, i) => {
  const size = (vertical ? 30 : 20) / colors.length;
  return vertical
    ? <rect key={fill + i} x={i * size} width={size} height={20} fill={fill} />
    : <rect key={fill + i} y={i * size} width={30} height={size} fill={fill} />;
});

// Desenhadas à mão em vez de emoji: o WebKit do Linux nem sempre tem fonte
// com bandeiras, e o seletor viraria uma fileira de letras soltas.
const FLAGS: Record<Locale, ReactNode> = {
  "pt-BR": <>
    <rect width={30} height={20} fill="#009c3b" />
    <polygon points="15,2.2 27.4,10 15,17.8 2.6,10" fill="#ffdf00" />
    <circle cx={15} cy={10} r={4.3} fill="#002776" />
    <path d="M10.9 9.1 Q15 7.9 19.2 10.8" stroke="#fff" strokeWidth={0.7} fill="none" />
  </>,
  en: <>
    {Array.from({ length: 13 }, (_, i) => <rect key={i} y={(i * 20) / 13} width={30} height={20 / 13} fill={i % 2 === 0 ? "#b22234" : "#fff"} />)}
    <rect width={13} height={(20 * 7) / 13} fill="#3c3b6e" />
    {Array.from({ length: 12 }, (_, i) => <circle key={i} cx={1.8 + (i % 4) * 3.1} cy={1.7 + Math.floor(i / 4) * 3.4} r={0.55} fill="#fff" />)}
  </>,
  es: <>
    <rect width={30} height={20} fill="#aa151b" />
    <rect y={5} width={30} height={10} fill="#f1bf00" />
  </>,
  "zh-CN": <>
    <rect width={30} height={20} fill="#de2910" />
    <polygon points={star(5, 5, 3)} fill="#ffde00" />
    {[[10, 2], [12, 4], [12, 7], [10, 9]].map(([x, y]) => <polygon key={`${x}-${y}`} points={star(x, y, 1)} fill="#ffde00" />)}
  </>,
  hi: <>
    {bands(["#ff9933", "#fff", "#138808"])}
    <circle cx={15} cy={10} r={2.6} fill="none" stroke="#000080" strokeWidth={0.6} />
    <circle cx={15} cy={10} r={0.6} fill="#000080" />
  </>,
  ar: <>
    <rect width={30} height={20} fill="#006c35" />
    {[7.5, 10.5, 13.5, 16.5, 19.5].map((x) => <rect key={x} x={x} y={5.5} width={1.8} height={4} rx={0.4} fill="#fff" />)}
    <rect x={7.5} y={11.4} width={14} height={0.9} fill="#fff" />
    <rect x={20.6} y={10.8} width={1.2} height={2.1} fill="#fff" />
  </>,
  fr: bands(["#002395", "#fff", "#ed2939"], true),
  ru: bands(["#fff", "#0039a6", "#d52b1e"]),
  ja: <>
    <rect width={30} height={20} fill="#fff" />
    <circle cx={15} cy={10} r={6} fill="#bc002d" />
  </>,
  de: bands(["#000", "#dd0000", "#ffce00"]),
};

/** A bandeira que acompanha cada idioma no seletor. */
export function Flag({ locale, className, ...props }: { locale: Locale } & SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 30 20" aria-hidden="true" className={cn("h-3.5 w-[21px] shrink-0 rounded-[2px] shadow-[0_0_0_1px_rgba(255,255,255,0.12)]", className)} {...props}>
      {FLAGS[locale]}
    </svg>
  );
}
