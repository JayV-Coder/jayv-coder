import type { ReactNode } from "react";

/** Uma vista que rola inteira, com a largura de leitura das telas de pátio. */
export function ScrollPage({ children }: { children: ReactNode }) {
  return <div className="flex-1 overflow-auto px-[max(38px,calc((100%-1080px)/2))] py-10">{children}</div>;
}
