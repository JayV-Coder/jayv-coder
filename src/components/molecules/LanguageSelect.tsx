import { LOCALES, setLocale, useLocale, useT, type Locale } from "@/modules/i18n";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

/** O idioma da interface. Cada idioma aparece com o próprio nome, para que
 * quem caiu numa língua que não lê ainda ache a sua. */
export function LanguageSelect() {
  const t = useT();
  const locale = useLocale();
  return (
    <Select value={locale} onValueChange={(value) => setLocale(value as Locale)}>
      <SelectTrigger size="sm" aria-label={t("language.label")} title={t("language.label")} className="w-full border-[#2b322d] bg-transparent text-xs text-[#8f9991] dark:bg-transparent">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {LOCALES.map((option) => <SelectItem key={option.id} value={option.id} lang={option.id}>{option.name}</SelectItem>)}
      </SelectContent>
    </Select>
  );
}
