import type { ReactNode } from "react";
import { AppHeader, CommandPalette, GeneralChatsColumn, Sidebar, StatusBar, useGeneralChatsColumn } from "@/components/organisms";
import { cn } from "@/lib/utils";

/** A moldura do aplicativo, como a janela de um terminal: lateral de 240px à
 * esquerda, cabeçalho e vista à direita e a barra de status no pé. No
 * ambiente de uma organização, os chats gerais dela ganham uma coluna própria
 * entre a lateral e a vista (220px; 184px na janela estreita, para a vista
 * não ficar espremida). A moldura ocupa a janela e nunca rola: a
 * lateral, a coluna, o cabeçalho e a barra ficam fixos, e só a vista rola por
 * baixo deles. A paleta de comandos (⌘K) mora aqui para valer em qualquer
 * tela. */
export function AppShell({ children }: { children: ReactNode }) {
  const general = useGeneralChatsColumn();
  return (
    <div className={cn(
      "relative grid h-full grid-rows-[minmax(0,1fr)_auto] overflow-hidden bg-background",
      general ? "grid-cols-[240px_184px_minmax(0,1fr)] lg:grid-cols-[240px_220px_minmax(0,1fr)]" : "grid-cols-[240px_minmax(0,1fr)]",
    )}>
      <Sidebar />
      {general && <GeneralChatsColumn />}
      <main className="flex min-h-0 min-w-0 flex-col">
        <AppHeader />
        {/* Uma vista sem rolagem própria rola aqui, sem levar a lateral junto.
            `relative` prende aqui dentro o que a vista posiciona: nada escapa
            para a janela. */}
        <div className="relative flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain">{children}</div>
      </main>
      <div className="col-span-full"><StatusBar /></div>
      <CommandPalette />
    </div>
  );
}
