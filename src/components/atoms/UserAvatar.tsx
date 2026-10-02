import { useState } from "react";
import { cn } from "@/lib/utils";

/** A foto da conta ou, sem ela, a inicial do nome sobre a superfície neutra. Uma
 * foto que não carrega (sem rede) cai na inicial em vez de virar ícone
 * quebrado. */
export function UserAvatar({ name, src, className }: { name: string; src?: string | null; className?: string }) {
  const [broken, setBroken] = useState(false);
  const initial = name.trim().charAt(0).toUpperCase() || "?";
  return (
    <span className={cn("grid size-9 shrink-0 place-items-center overflow-hidden rounded-full border border-border bg-secondary font-semibold text-foreground", className)}>
      {src && !broken
        ? <img src={src} alt="" referrerPolicy="no-referrer" onError={() => setBroken(true)} className="size-full object-cover" />
        : <span aria-hidden="true">{initial}</span>}
    </span>
  );
}
