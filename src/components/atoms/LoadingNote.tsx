import type { ReactNode } from "react";

/** O aviso de espera, no meio da vista que ainda não chegou. */
export function LoadingNote({ children }: { children: ReactNode }) {
  return <div className="flex flex-1 items-center justify-center p-10 text-sm text-muted-foreground" role="status">{children}</div>;
}
