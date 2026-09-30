import type { ReactNode } from "react";
import { Card } from "@/components/ui/card";

/** O cartão dos pátios de projetos e de chats: a parte grande abre, os botões
 * de baixo agem. O cartão ocupa a altura da linha da grade e o rodapé fica
 * sempre colado embaixo, alinhado com o dos vizinhos. */
export function YardCard({ onOpen, actions, children }: { onOpen: () => void; actions: ReactNode; children: ReactNode }) {
  return (
    <Card className="h-full gap-0 overflow-hidden py-0 transition-colors hover:border-[#3d4a40]">
      <button type="button" onClick={onOpen} className="grid flex-1 content-start gap-2.5 px-5 pt-5 pb-4 text-start">{children}</button>
      <div className="mt-auto flex items-center gap-2 border-t border-border px-4 py-2.5">{actions}</div>
    </Card>
  );
}
