import type { ReactNode } from "react";
import { AppHeader, Sidebar } from "@/components/organisms";

/** A moldura do aplicativo: lateral de 240px à esquerda, cabeçalho e vista à
 * direita. Cada região rola sozinha. */
export function AppShell({ children }: { children: ReactNode }) {
  return (
    <div className="grid h-screen grid-cols-[240px_minmax(0,1fr)] bg-background">
      <Sidebar />
      <main className="flex h-screen min-w-0 flex-col">
        <AppHeader />
        <div className="flex min-h-0 flex-1 flex-col">{children}</div>
      </main>
    </div>
  );
}
