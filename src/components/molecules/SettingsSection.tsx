import type { ReactNode } from "react";
import { Card } from "@/components/ui/card";

/** Um bloco da aba de um agente: título, o que ele controla e os campos. */
export function SettingsSection({ title, description, action, children }: { title: string; description?: string; action?: ReactNode; children: ReactNode }) {
  return (
    <Card className="gap-5 px-6 py-5">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h3 className="text-base font-semibold">{title}</h3>
          {description && <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>}
        </div>
        {action}
      </div>
      {children}
    </Card>
  );
}
