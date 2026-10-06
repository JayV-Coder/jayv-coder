import { useEffect, useState } from "react";
import type { TurnEvidence } from "@/modules/core";
import { reportError } from "@/modules/feedback";
import { evidenceGroups, loadEvidence } from "@/modules/gate";
import { useT, type Key } from "@/modules/i18n";
import { LoadingNote } from "@/components/atoms";

/** O que foi conferido numa resposta, aberto pelo botão do balão. Os três
 * grupos ficam separados de propósito: o que o JayV observou, o que um modelo
 * disse e ninguém conferiu, e o que ficou sem verificação. Nada aqui é verde:
 * passar pelas regras da casa não prova que a mudança está certa. Cada
 * abertura relê o banco, e a segunda opinião que chegou depois aparece. */
export function EvidencePanel({ turnId }: { turnId: string }) {
  const t = useT();
  const [evidence, setEvidence] = useState<TurnEvidence | null>(null);
  useEffect(() => {
    let live = true;
    loadEvidence(turnId).then((found) => { if (live) setEvidence(found); }, reportError);
    return () => { live = false; };
  }, [turnId]);

  if (!evidence) return <LoadingNote fill={false}>{t("evidence.loading")}</LoadingNote>;
  const groups = evidenceGroups(evidence, t);
  const sections: [Key, string[]][] = [
    ["evidence.observed", groups.observed],
    ["evidence.inferred", groups.inferred],
    ["evidence.unverified", groups.unverified],
  ];
  return (
    <section aria-label={t("evidence.toggle")} className="grid gap-2.5 rounded-sm border border-border bg-secondary/40 px-3 py-2.5 text-small">
      {sections.map(([title, lines]) => (
        <div key={title} className="grid gap-1">
          <h4 className="text-caption font-semibold tracking-wide text-dim uppercase">{t(title)}</h4>
          <ul className="grid gap-0.5">
            {lines.map((line) => (
              <li key={line} className="flex gap-2 leading-snug [overflow-wrap:anywhere] before:shrink-0 before:text-faint before:content-['–']">{line}</li>
            ))}
          </ul>
        </div>
      ))}
      <p className="text-caption text-muted-foreground">{t("evidence.disclaimer")}</p>
    </section>
  );
}
