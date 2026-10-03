import type { ReactNode } from "react";
import { AppHeader, Sidebar } from "@/components/organisms";

/** A moldura do aplicativo: lateral de 240px à esquerda, cabeçalho e vista à
 * direita. A moldura ocupa a janela e nunca rola: a lateral e o cabeçalho
 * ficam fixos, e só a vista rola por baixo deles. */
export function AppShell({ children }: { children: ReactNode }) {
  return (
    <div className="grid h-full grid-cols-[240px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)] overflow-hidden bg-background">
      <Sidebar />
      <main className="flex min-h-0 min-w-0 flex-col">
        <AppHeader />
        {/* Uma vista sem rolagem própria rola aqui, sem levar a lateral junto. */}
        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">{children}</div>
      </main>
    </div>
  );
}
