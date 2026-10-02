import { useMemo, useState } from "react";
import { XIcon } from "lucide-react";
import { useLocale, useT } from "@/modules/i18n";
import { daysInMonth, joinDate, splitDate, type DateParts as Parts } from "@/modules/profile";
import { Button } from "@/components/ui/button";
import { OptionSelect, type Option } from "./OptionSelect";

const UNSET = "unset";
const FIRST_YEAR = 1900;

/** Uma data distante (nascimento) em três escolhas: dia, mês por extenso e
 * ano. O calendário nativo obriga a voltar mês a mês até décadas atrás. A
 * data só sai quando as três partes estão escolhidas; o dia se ajusta ao mês
 * e ao ano (fevereiro de ano bissexto). */
export function DateParts({ id, value, onChange }: { id?: string; value: string | null; onChange: (value: string | null) => void }) {
  const t = useT();
  const locale = useLocale();
  // As partes soltas ficam aqui até a data ficar completa.
  const [parts, setParts] = useState<Parts>(() => splitDate(value));

  const now = new Date();
  const thisYear = now.getFullYear();
  const unset = { value: UNSET, label: "—" };
  const months = useMemo<Option<string>[]>(() => {
    const name = new Intl.DateTimeFormat(locale, { month: "long", timeZone: "UTC" });
    return Array.from({ length: 12 }, (_, index) => ({ value: String(index + 1), label: name.format(new Date(Date.UTC(2000, index, 1))) }));
  }, [locale]);
  const years = useMemo<Option<string>[]>(() =>
    Array.from({ length: thisYear - FIRST_YEAR + 1 }, (_, index) => String(thisYear - index)).map((year) => ({ value: year, label: year })), [thisYear]);
  const days = Array.from({ length: daysInMonth(parts.year, parts.month) }, (_, index) => String(index + 1)).map((day) => ({ value: day, label: day }));

  const update = (change: Partial<Parts>) => {
    const next = { ...parts, ...change };
    // Trocar o mês ou o ano pode tirar o dia do calendário (31 de abril).
    if (next.day !== null && next.day > daysInMonth(next.year, next.month)) next.day = null;
    // Hoje é o limite: um dia futuro do ano atual cai fora.
    const joined = joinDate(next);
    if (joined && joined > now.toISOString().slice(0, 10)) next.day = null;
    setParts(next);
    onChange(joinDate(next));
  };
  const read = (text: string) => (text === UNSET ? null : Number(text));
  const filled = parts.year !== null || parts.month !== null || parts.day !== null;

  return (
    <div className="flex items-center gap-2">
      <div className="w-[4.5rem] shrink-0">
        <OptionSelect id={id} value={parts.day === null ? UNSET : String(parts.day)} label={t("profile.birthDate.day")} options={[unset, ...days]} onChange={(day) => update({ day: read(day) })} />
      </div>
      <div className="min-w-0 flex-1">
        <OptionSelect id={id ? `${id}-month` : undefined} value={parts.month === null ? UNSET : String(parts.month)} label={t("profile.birthDate.month")} options={[unset, ...months]} onChange={(month) => update({ month: read(month) })} />
      </div>
      <div className="w-[5.5rem] shrink-0">
        <OptionSelect id={id ? `${id}-year` : undefined} value={parts.year === null ? UNSET : String(parts.year)} label={t("profile.birthDate.year")} options={[unset, ...years]} onChange={(year) => update({ year: read(year) })} />
      </div>
      <Button type="button" variant="ghost" size="icon" aria-label={t("profile.birthDate.clear")} title={t("profile.birthDate.clear")} disabled={!filled}
        onClick={() => update({ year: null, month: null, day: null })}>
        <XIcon />
      </Button>
    </div>
  );
}
