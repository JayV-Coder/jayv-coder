import type { Criterion } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { cn } from "@/lib/utils";

/** A escala do tamanho, na ordem e na grafia em que o núcleo a devolve. */
export const SCALE = ["ajuste pequeno", "funcionalidade", "sistema inteiro"];
const SCALE_KEYS: Key[] = ["scope.0", "scope.1", "scope.2"];

/** O tamanho do pedido no idioma de quem lê. O núcleo o devolve em português. */
export function scopeKey(scope: string): Key | null {
  const at = SCALE.indexOf(scope);
  return at < 0 ? null : SCALE_KEYS[at];
}

const CRITERIA: Record<string, [Key, Key, Key]> = {
  goal_is_clear: ["criterion.goal_is_clear", "criterion.goal_is_clear.in", "criterion.goal_is_clear.out"],
  says_where: ["criterion.says_where", "criterion.says_where.in", "criterion.says_where.out"],
  says_when_done: ["criterion.says_when_done", "criterion.says_when_done.in", "criterion.says_when_done.out"],
  bundles_requests: ["criterion.bundles_requests", "criterion.bundles_requests.in", "criterion.bundles_requests.out"],
};

function withinBand(criterion: Criterion) {
  return !criterion.band || (criterion.percent >= criterion.band[0] && criterion.percent <= criterion.band[1]);
}

/** Um critério da portaria: barrinha de 0 a 100% com a faixa de tolerância
 * desenhada atrás. Fora da faixa, o critério ganha um risco na cor do
 * veredito. */
export function Gauge({ criterion }: { criterion: Criterion }) {
  const t = useT();
  const outside = !withinBand(criterion);
  // O nome e a leitura vêm do núcleo em português; os critérios conhecidos são
  // ditos de novo no idioma de quem lê, a partir do id e de estar ou não na faixa.
  const known = CRITERIA[criterion.id];
  const label = criterion.id === "scope" ? t("criterion.scope") : known ? t(known[0]) : criterion.label;
  const reading = known ? t(outside ? known[2] : known[1]) : criterion.reading;
  return (
    <div className="relative grid grid-cols-[150px_1fr_48px] items-center gap-x-[15px] gap-y-[5px] border-t border-[#12161a] py-2.5 xl:grid-cols-[196px_1fr_52px]">
      {outside && <span className="absolute top-2.5 bottom-2.5 -start-2.5 w-0.5 bg-[var(--aspect)] xl:-start-[17px]" />}
      <span className={cn("text-sm text-[#c6ccd3]", outside && "text-[#eef1f4]")}>{label}</span>
      <span className="relative h-[11px] border border-[#1e242a] bg-[#11151a]">
        {criterion.band && (
          <span
            className="absolute inset-y-0 z-[2] border-x border-[#9aa5b0] bg-[repeating-linear-gradient(135deg,#ffffff14_0_3px,transparent_3px_6px)]"
            style={{ left: `${criterion.band[0]}%`, width: `${criterion.band[1] - criterion.band[0]}%` }}
          />
        )}
        <span className="absolute inset-y-0 start-0 z-[1] bg-[#59646f]" style={{ width: `${criterion.percent}%` }} />
        <span className="absolute -top-[3px] -bottom-[3px] z-[3] -ms-px w-0.5 bg-[#eef1f4]" style={{ insetInlineStart: `${criterion.percent}%` }} />
      </span>
      <span className="text-end font-gate-mono text-[13.5px] text-[#eef1f4] tabular-nums">{criterion.percent}%</span>
      {criterion.id === "scope" ? (
        <span className="col-[2/4] flex justify-between gap-2.5 text-[11px] whitespace-nowrap text-faint">
          {SCALE.map((level, at) => <i key={level} className={cn("not-italic", level === criterion.reading && "text-[#eef1f4]")}>{t(SCALE_KEYS[at])}</i>)}
        </span>
      ) : (
        <span className="col-[2/4] text-[12.5px] text-dim">{reading}</span>
      )}
    </div>
  );
}
