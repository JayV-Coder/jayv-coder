import type { ReactNode } from "react";
import { BackMark, Eyebrow } from "@/components/atoms";
import { Button } from "@/components/ui/button";

/** Título de uma vista: onde se está, o nome dela depois do prompt (`❯`) e,
 * à direita, o que se pode fazer nela. Uma vista de dentro (uma organização)
 * troca o "onde se está" pela marca de voltar, colada ao título. */
export function PageHeading({ eyebrow, back, title, description, children }: {
  eyebrow?: string;
  back?: { label: string; onClick: () => void };
  title: string;
  description?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0">
        {eyebrow && <Eyebrow>{eyebrow}</Eyebrow>}
        <div className="flex items-center gap-2">
          {back && (
            <Button variant="ghost" size="icon-sm" aria-label={back.label} title={back.label} onClick={back.onClick} className="-ml-2">
              <BackMark />
            </Button>
          )}
          <h2 className="flex min-w-0 gap-[1ch] text-h2 font-semibold"><span aria-hidden="true" className="text-go">❯</span><span className="break-words">{title}</span></h2>
        </div>
        {description && <p className="mt-1 max-w-[62ch] text-sm leading-relaxed text-muted-foreground">{description}</p>}
      </div>
      {children && <div className="flex flex-wrap items-center gap-2">{children}</div>}
    </div>
  );
}
