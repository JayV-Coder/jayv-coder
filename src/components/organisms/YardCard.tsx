import type { ReactNode } from "react";
import { Card } from "@/components/ui/card";

/** O cartão dos pátios de projetos e de chats: a parte grande abre, os botões
 * de baixo agem. */
export function YardCard({ onOpen, actions, children }: { onOpen: () => void; actions: ReactNode; children: ReactNode }) {
  return (
    <Card className="gap-0 overflow-hidden py-0 transition-colors hover:border-[#3d4a40]">
      <button type="button" onClick={onOpen} className="grid gap-2.5 px-5 pt-5 pb-4 text-start">{children}</button>
      <div className="flex gap-2 border-t border-border px-4 py-2.5">{actions}</div>
    </Card>
  );
}
