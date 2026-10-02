import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { CheckIcon, ChevronsUpDownIcon, SearchIcon } from "lucide-react";
import { setLocale, useLocale, useLocales, useT, type Locale } from "@/modules/i18n";
import { Flag } from "@/components/atoms";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";

/** Sem acento e sem caixa: "portugues" acha "Português". */
const fold = (text: string) => text.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();

/** O idioma da interface. Cada idioma aparece com o próprio nome e, embaixo,
 * no idioma atual; a busca aceita os dois, o inglês e o código. */
export function LanguageSelect() {
  const t = useT();
  const locale = useLocale();
  const locales = useLocales();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const list = useRef<HTMLDivElement>(null);

  const options = useMemo(() => {
    const here = new Intl.DisplayNames([locale], { type: "language" });
    const english = new Intl.DisplayNames(["en"], { type: "language" });
    return locales.map((option) => {
      const translated = here.of(option.id) ?? option.name;
      return { id: option.id as Locale, name: option.name, translated, search: fold([option.name, translated, english.of(option.id), option.id].join(" ")) };
    });
  }, [locale, locales]);
  const shown = options.filter((option) => option.search.includes(fold(query.trim())));
  const current = options.find((option) => option.id === locale) ?? options[0];

  useEffect(() => { setActive(Math.max(0, shown.findIndex((option) => option.id === locale))); }, [query, open]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { list.current?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" }); }, [active]);

  const choose = (id: Locale) => {
    setLocale(id);
    setOpen(false);
  };
  const onKeyDown = (event: KeyboardEvent) => {
    const last = shown.length - 1;
    const moves: Record<string, number> = { ArrowDown: Math.min(active + 1, last), ArrowUp: Math.max(active - 1, 0), Home: 0, End: last };
    if (event.key in moves) {
      event.preventDefault();
      setActive(moves[event.key]);
    } else if (event.key === "Enter" && shown[active]) {
      event.preventDefault();
      choose(shown[active].id);
    }
  };

  return (
    <Popover open={open} onOpenChange={(next) => { setOpen(next); if (next) setQuery(""); }}>
      <PopoverTrigger
        aria-label={t("language.label")}
        title={t("language.label")}
        className="flex h-10 w-full items-center gap-2 rounded-md border border-input bg-card px-3 text-sm text-foreground transition-colors outline-none hover:border-muted-foreground/50 focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/20"
      >
        <Flag locale={locale} />
        <span className="min-w-0 flex-1 truncate text-start" lang={locale}>{current.name}</span>
        <ChevronsUpDownIcon className="size-3.5 opacity-60" />
      </PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-(--radix-popover-trigger-width) min-w-60 p-0">
        <div className="flex items-center gap-2 border-b px-3">
          <SearchIcon className="size-3.5 shrink-0 text-muted-foreground" />
          <input
            autoFocus
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={onKeyDown}
            placeholder={t("language.search")}
            aria-label={t("language.search")}
            aria-controls="language-list"
            aria-activedescendant={shown[active] ? `language-${shown[active].id}` : undefined}
            className="h-9 w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
        </div>
        <div ref={list} id="language-list" role="listbox" aria-label={t("language.label")} className="max-h-72 overflow-y-auto overscroll-contain p-1">
          {shown.length === 0 && <p className="px-2 py-6 text-center text-xs text-muted-foreground">{t("language.none")}</p>}
          {shown.map((option, index) => (
            <div
              key={option.id}
              id={`language-${option.id}`}
              role="option"
              aria-selected={option.id === locale}
              data-index={index}
              onPointerMove={() => setActive(index)}
              onClick={() => choose(option.id)}
              className={cn("flex cursor-default items-center gap-2.5 rounded-sm px-2 py-1.5", index === active && "bg-secondary text-foreground")}
            >
              <Flag locale={option.id} />
              <span className="grid min-w-0 flex-1">
                <span className="truncate text-sm" lang={option.id}>{option.name}</span>
                {option.translated !== option.name && <span className="truncate text-caption text-muted-foreground">{option.translated}</span>}
              </span>
              {option.id === locale && <CheckIcon className="size-4 shrink-0" />}
            </div>
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}
