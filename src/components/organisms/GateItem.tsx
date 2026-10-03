import type { ReactNode } from "react";
import type { Aspect } from "@/modules/core";
import { settleArrival, useGate } from "@/modules/gate";
import { cn } from "@/lib/utils";

/** A casca comum dos cartões da portaria: cor do veredito, a margem pintada
 * como num bloco do terminal (âmbar ou vermelho; verde não pinta) e a batida
 * de lâmpada quando o item acabou de chegar. */
export function GateItem({ id, aspect, children }: { id: string; aspect: Aspect; children: ReactNode }) {
  const arriving = useGate((state) => Boolean(state.arriving[id]));
  return (
    <article
      data-aspect={aspect}
      data-arriving={arriving || undefined}
      onAnimationEnd={() => arriving && settleArrival(id)}
      className={cn("relative border-b border-rail before:absolute before:inset-y-0 before:start-0 before:w-[3px] data-[aspect=ask]:before:bg-ask data-[aspect=stop]:before:bg-stop", arriving && "animate-gate-strike motion-reduce:animate-none")}
    >
      {children}
    </article>
  );
}
