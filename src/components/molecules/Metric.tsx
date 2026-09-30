import type { ReactNode } from "react";
import { Card } from "@/components/ui/card";

/** Um número do sistema, com o nome em cima. */
export function Metric({ label, children, title }: { label: string; children: ReactNode; title?: string }) {
  return (
    <Card className="gap-2 px-5 py-5">
      <small className="text-xs text-muted-foreground">{label}</small>
      <strong title={title} className="truncate text-xl">{children}</strong>
    </Card>
  );
}
