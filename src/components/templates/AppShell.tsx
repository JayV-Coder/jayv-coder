import type { ReactNode } from "react";
import { AppHeader, Sidebar } from "@/components/organisms";

/** A moldura do aplicativo: lateral à esquerda, cabeçalho e vista à direita. */
export function AppShell({ children }: { children: ReactNode }) {
  return (
    <div className="grid min-h-screen grid-cols-[260px_1fr]">
      <Sidebar />
      <main className="flex h-screen min-w-0 flex-col">
        <AppHeader />
        <div className="flex min-h-0 flex-1 flex-col">{children}</div>
      </main>
    </div>
  );
}
