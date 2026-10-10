import type { ReactNode } from "react";

/** Uma vista que rola inteira, com 24px de margem e a largura de leitura das
 * telas de pátio. O que ela posiciona rola junto e não escapa para a janela. */
export function ScrollPage({ children }: { children: ReactNode }) {
  return (
    <div className="relative min-h-0 flex-1 overflow-auto overscroll-contain p-6">
      <div className="mx-auto w-full max-w-[1080px]">{children}</div>
    </div>
  );
}
