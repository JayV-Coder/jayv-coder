import type { ReactNode } from "react";
import { useT } from "@/modules/i18n";
import { cn } from "@/lib/utils";

/** Uma das duas faixas da portaria: o portão, o que ele faz e o feed dele. */
export function GateLane({ icon, title, description, count, waiting, empty, children }: {
  icon: ReactNode;
  title: string;
  description: string;
  count: number;
  waiting?: boolean;
  empty: string;
  children: ReactNode[];
}) {
  const t = useT();
  const counted = count ? t("gate.lane.count", { count }) : "";
  const waitingText = t("gate.lane.waiting");
  return (
    <section className="grid min-h-0 min-w-0 grid-rows-[auto_1fr] [&+&]:border-t [&+&]:border-rail-2 xl:[&+&]:border-t-0 xl:[&+&]:border-s">
      <div className="grid grid-cols-[auto_1fr_auto] items-center gap-x-[11px] gap-y-0.5 border-b border-rail-2 bg-sidebar px-[22px] py-2.5">
        <span className="row-span-2 size-[19px] text-muted-foreground">{icon}</span>
        <h2 className="text-h4 font-semibold text-foreground">{title}</h2>
        <span className={cn("font-gate-mono text-xs whitespace-nowrap text-faint", waiting && "animate-gate-wait text-foreground motion-reduce:animate-none")}>
          {waiting ? (counted ? `${waitingText} · ${counted}` : waitingText) : counted}
        </span>
        <p className="col-[2/4] text-sm leading-[1.35] text-dim">{description}</p>
      </div>
      <div className="min-h-0 overflow-auto">
        {children.length === 0 ? <p className="max-w-[48ch] px-[22px] py-7 text-sm leading-[1.7] text-dim">{empty}</p> : children}
      </div>
    </section>
  );
}
