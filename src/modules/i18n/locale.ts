export interface LocaleChoice { id: string; name: string }

/** O idioma da interface: o salvo, se o JayV o sabe falar; senão o primeiro que
 * o sistema pede e existe na lista (`pt-PT` cai em `pt-BR`, `zh-TW` em
 * `zh-CN`); senão o `fallback`. Com a lista de idiomas que não chegou (sem
 * rede, antes do login), o salvo vale mesmo assim: é a escolha da pessoa, e
 * trocá-lo por inglês faria o JayV responder em inglês. */
export function resolveLocale(options: { saved: string | null; locales: LocaleChoice[]; listed: boolean; system: readonly string[]; fallback: string }): string {
  const { saved, locales, listed, system, fallback } = options;
  if (saved && (!listed || locales.some((locale) => locale.id === saved))) return saved;
  for (const wanted of system) {
    const exact = locales.find((locale) => locale.id.toLowerCase() === wanted.toLowerCase());
    if (exact) return exact.id;
    const language = wanted.split("-")[0].toLowerCase();
    const near = locales.find((locale) => locale.id.split("-")[0].toLowerCase() === language);
    if (near) return near.id;
  }
  return fallback;
}

/** O nome do idioma que o modelo lê ("responda em Português"): o da lista; sem
 * ele, o que o próprio navegador sabe dizer do código. */
export function replyName(locale: string, locales: LocaleChoice[]): string {
  const known = locales.find((item) => item.id === locale)?.name;
  if (known) return known;
  try {
    return new Intl.DisplayNames(["en"], { type: "language" }).of(locale) ?? locale;
  } catch {
    return locale;
  }
}
