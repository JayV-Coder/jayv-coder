import type { ReactNode } from "react";
import type { Aspect } from "@/modules/core";
import { settleArrival, useGate } from "@/modules/gate";
import { cn } from "@/lib/utils";

/** A casca comum dos cartões da portaria: cor do veredito e a batida de
 * lâmpada quando o item acabou de chegar. */
export function GateItem({ id, aspect, children }: { id: string; aspect: Aspect; children: ReactNode }) {
  const arriving = useGate((state) => Boolean(state.arriving[id]));
  return (
    <article
      data-aspect={aspect}
      data-arriving={arriving || undefined}
      onAnimationEnd={() => arriving && settleArrival(id)}
      className={cn("border-b border-rail", arriving && "animate-gate-strike motion-reduce:animate-none")}
    >
      {children}
    </article>
  );
}
