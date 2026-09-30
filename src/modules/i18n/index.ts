import { useCallback } from "react";
import { create } from "zustand";
import type { Key, Message, Messages } from "./types";
import { ptBR } from "./messages/pt-BR";
import { en } from "./messages/en";
import { es } from "./messages/es";
import { zhCN } from "./messages/zh-CN";
import { hi } from "./messages/hi";
import { ar } from "./messages/ar";
import { fr } from "./messages/fr";
import { ru } from "./messages/ru";
import { ja } from "./messages/ja";
import { de } from "./messages/de";

export type { Key, Message, Messages, Plural } from "./types";

/** Os idiomas mais falados, cada um com o nome que o próprio falante reconhece
 * no seletor. */
export const LOCALES = [
  { id: "pt-BR", name: "Português (Brasil)", messages: ptBR as Messages },
  { id: "en", name: "English", messages: en },
  { id: "es", name: "Español", messages: es },
  { id: "zh-CN", name: "简体中文", messages: zhCN },
  { id: "hi", name: "हिन्दी", messages: hi },
  { id: "ar", name: "العربية", messages: ar, rtl: true },
  { id: "fr", name: "Français", messages: fr },
  { id: "ru", name: "Русский", messages: ru },
  { id: "ja", name: "日本語", messages: ja },
  { id: "de", name: "Deutsch", messages: de },
] as const;

export type Locale = (typeof LOCALES)[number]["id"];
export type Params = Record<string, string | number>;

const LOCALE_KEY = "jayv.locale";
const FALLBACK: Locale = "en";

function known(id: string | null): Locale | null {
  return LOCALES.find((locale) => locale.id === id)?.id ?? null;
}

/** O idioma salvo, ou o primeiro que o sistema pede e o JayV sabe falar —
 * `pt-PT` cai em `pt-BR`, `zh-TW` em `zh-CN` —, ou inglês. */
function detect(): Locale {
  const saved = known(localStorage.getItem(LOCALE_KEY));
  if (saved) return saved;
  for (const wanted of navigator.languages ?? [navigator.language]) {
    const exact = LOCALES.find((locale) => locale.id.toLowerCase() === wanted.toLowerCase());
    if (exact) return exact.id;
    const language = wanted.split("-")[0].toLowerCase();
    const near = LOCALES.find((locale) => locale.id.split("-")[0].toLowerCase() === language);
    if (near) return near.id;
  }
  return FALLBACK;
}

function apply(locale: Locale) {
  const entry = LOCALES.find((known) => known.id === locale)!;
  document.documentElement.lang = locale;
  document.documentElement.dir = "rtl" in entry && entry.rtl ? "rtl" : "ltr";
}

interface I18nState {
  locale: Locale;
}

export const useI18n = create<I18nState>(() => ({ locale: detect() }));
apply(useI18n.getState().locale);

export function setLocale(locale: Locale) {
  localStorage.setItem(LOCALE_KEY, locale);
  apply(locale);
  useI18n.setState({ locale });
}

export function translate(locale: Locale, key: Key, params?: Params): string {
  const message: Message = LOCALES.find((known) => known.id === locale)?.messages[key] ?? ptBR[key];
  const text = typeof message === "string"
    ? message
    : message[new Intl.PluralRules(locale).select(Number(params?.count ?? 0))] ?? message.other;
  return text.replace(/\{(\w+)\}/g, (mark, name: string) => (params && name in params ? String(params[name]) : mark));
}

/** O texto no idioma atual, para quem não é componente: avisos e estados que
 * os módulos guardam. */
export function t(key: Key, params?: Params) {
  return translate(useI18n.getState().locale, key, params);
}

/** O texto no idioma atual, para componentes: trocar de idioma redesenha quem
 * usa. */
export function useT() {
  const locale = useI18n((state) => state.locale);
  return useCallback((key: Key, params?: Params) => translate(locale, key, params), [locale]);
}

export function useLocale() {
  return useI18n((state) => state.locale);
}

export function formatSince(iso: string, locale = useI18n.getState().locale) {
  return new Intl.DateTimeFormat(locale, { day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit" }).format(new Date(iso));
}

export function formatClock(iso: string, locale = useI18n.getState().locale) {
  const at = new Date(iso);
  return Number.isNaN(at.valueOf()) ? "--:--:--" : new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }).format(at);
}
