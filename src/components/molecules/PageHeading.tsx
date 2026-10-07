import type { ReactNode } from "react";
import { BackMark, Eyebrow } from "@/components/atoms";

/** Título de uma vista: onde se está, o nome dela depois do prompt (`❯`) e,
 * à direita, o que se pode fazer nela. Uma vista de dentro (uma organização)
 * troca o "onde se está" pelo voltar do site: a marca virada para a esquerda
 * e o nome da vista de onde se veio, numa linha acima do título. */
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
        {back && (
          <button type="button" onClick={back.onClick} className="mb-4 inline-flex items-center gap-[1ch] text-sm text-muted-foreground transition-colors hover:text-foreground">
            <BackMark />
            <span>{back.label}</span>
          </button>
        )}
        <div className="flex items-center gap-2">
          <h2 className="flex min-w-0 gap-[1ch] text-h2 font-semibold"><span aria-hidden="true" className="text-go">❯</span><span className="break-words">{title}</span></h2>
        </div>
        {description && <p className="mt-1 max-w-[62ch] text-sm leading-relaxed text-muted-foreground">{description}</p>}
      </div>
      {children && <div className="flex flex-wrap items-center gap-2">{children}</div>}
    </div>
  );
}
