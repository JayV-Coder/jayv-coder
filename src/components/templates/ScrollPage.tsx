import type { ReactNode } from "react";

/** Uma vista que rola inteira, com 24px de margem e a largura de leitura das
 * telas de pátio. */
export function ScrollPage({ children }: { children: ReactNode }) {
  return (
    <div className="flex-1 overflow-auto p-6">
      <div className="mx-auto w-full max-w-[1080px]">{children}</div>
    </div>
  );
}
