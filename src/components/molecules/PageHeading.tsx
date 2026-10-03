import type { ReactNode } from "react";
import { Eyebrow } from "@/components/atoms";

/** Título de uma vista: onde se está, o nome dela depois do prompt (`❯`) e,
 * à direita, o que se pode fazer nela. */
export function PageHeading({ eyebrow, title, description, children }: { eyebrow: string; title: string; description?: ReactNode; children?: ReactNode }) {
  return (
    <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0">
        <Eyebrow>{eyebrow}</Eyebrow>
        <h2 className="flex gap-[1ch] text-h2 font-semibold"><span aria-hidden="true" className="text-go">❯</span>{title}</h2>
        {description && <p className="mt-1 max-w-[62ch] text-sm leading-relaxed text-muted-foreground">{description}</p>}
      </div>
      {children && <div className="flex items-center gap-2">{children}</div>}
    </div>
  );
}
