import { useCallback } from "react";
import { create } from "zustand";
import { commands, onCore } from "@/modules/core/bridge";
import type { Key, Message, Messages } from "./types";
import { en } from "./messages/en";

export type { Key, Message, Messages, Plural } from "./types";

export type Locale = string;
export interface LocaleOption { id: Locale; name: string; rtl?: boolean }

/** O inglês vem no build; os outros idiomas chegam do Supabase pelo Rust. */
const BUILT_IN: LocaleOption[] = [{ id: "en", name: "English" }];
const LOCALE_KEY = "jayv.locale";
const FALLBACK: Locale = "en";

interface I18nState {
  locale: Locale;
  locales: LocaleOption[];
  /** As mensagens do idioma atual; o que faltar sai de `en`. */
  messages: Messages;
}

/** O idioma salvo, ou o primeiro que o sistema pede e o JayV sabe falar —
 * `pt-PT` cai em `pt-BR`, `zh-TW` em `zh-CN` —, ou inglês. */
function detect(locales: LocaleOption[]): Locale {
  const saved = localStorage.getItem(LOCALE_KEY);
  if (saved && locales.some((locale) => locale.id === saved)) return saved;
  for (const wanted of navigator.languages ?? [navigator.language]) {
    const exact = locales.find((locale) => locale.id.toLowerCase() === wanted.toLowerCase());
    if (exact) return exact.id;
    const language = wanted.split("-")[0].toLowerCase();
    const near = locales.find((locale) => locale.id.split("-")[0].toLowerCase() === language);
    if (near) return near.id;
  }
  return FALLBACK;
}

function apply(locale: Locale, locales: LocaleOption[]) {
  document.documentElement.lang = locale;
  document.documentElement.dir = locales.find((known) => known.id === locale)?.rtl ? "rtl" : "ltr";
}

/** Antes da lista chegar, vale o idioma salvo: sem isso a tela piscaria em
 * inglês a cada abertura de quem escolheu outro. */
export const useI18n = create<I18nState>(() => ({
  locale: localStorage.getItem(LOCALE_KEY) ?? FALLBACK,
  locales: BUILT_IN,
  messages: {},
}));

async function loadMessages(locale: Locale) {
  const messages = locale === FALLBACK ? {} : await commands.getTranslations(locale).catch(() => ({}));
  if (useI18n.getState().locale === locale) useI18n.setState({ messages: messages as Messages });
}

async function loadLocales() {
  const fetched = await commands.getLocales().catch(() => []);
  const locales = fetched.length ? fetched : BUILT_IN;
  const locale = detect(locales);
  useI18n.setState({ locales, locale });
  apply(locale, locales);
  await loadMessages(locale);
}

/** Pede ao núcleo os idiomas e as traduções, e pede de novo quando o cache
 * dele recebe novidades do Supabase. */
export function connectI18n() {
  void loadLocales();
  const off = onCore("translations-updated", () => void loadLocales());
  return () => void off.then((unlisten) => unlisten());
}

export function setLocale(locale: Locale) {
  localStorage.setItem(LOCALE_KEY, locale);
  apply(locale, useI18n.getState().locales);
  useI18n.setState({ locale, messages: {} });
  void loadMessages(locale);
}

export type Params = Record<string, string | number>;

function render(locale: Locale, messages: Messages, key: Key, params?: Params): string {
  const message: Message | undefined = messages[key] ?? en[key];
  // A chave veio do núcleo e esta versão da tela ainda não a conhece.
  if (message === undefined) return key;
  const text = typeof message === "string"
    ? message
    : message[new Intl.PluralRules(locale).select(Number(params?.count ?? 0))] ?? message.other;
  return text.replace(/\{(\w+)\}/g, (mark, name: string) => (params && name in params ? String(params[name]) : mark));
}

export function translate(locale: Locale, key: Key, params?: Params): string {
  const state = useI18n.getState();
  return render(locale, state.locale === locale ? state.messages : {}, key, params);
}

/** O texto no idioma atual, para quem não é componente: avisos e estados que
 * os módulos guardam. */
export function t(key: Key, params?: Params) {
  return translate(useI18n.getState().locale, key, params);
}

/** O texto que o núcleo devolve no lugar de uma frase: a chave e os valores
 * que ela cita, que podem ser outras chaves (ver `src-tauri/src/i18n.rs`). */
export interface Text { key: string; params?: Record<string, string | Text> }

export function isText(value: unknown): value is Text {
  return typeof value === "object" && value !== null && typeof (value as Text).key === "string";
}

/** O texto do núcleo no idioma atual, com os valores que também são chaves
 * traduzidos antes. */
export function say(text: Text): string {
  const params: Params = {};
  for (const [name, value] of Object.entries(text.params ?? {})) params[name] = typeof value === "string" ? value : say(value);
  return t(text.key as Key, params);
}

/** O texto no idioma atual, para componentes: trocar de idioma — ou chegarem
 * as traduções dele — redesenha quem usa. */
export function useT() {
  const locale = useI18n((state) => state.locale);
  const messages = useI18n((state) => state.messages);
  return useCallback((key: Key, params?: Params) => render(locale, messages, key, params), [locale, messages]);
}

export function useLocale() {
  return useI18n((state) => state.locale);
}

export function useLocales() {
  return useI18n((state) => state.locales);
}

export function formatSince(iso: string, locale = useI18n.getState().locale) {
  return new Intl.DateTimeFormat(locale, { day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit" }).format(new Date(iso));
}

export function formatClock(iso: string, locale = useI18n.getState().locale) {
  const at = new Date(iso);
  return Number.isNaN(at.valueOf()) ? "--:--:--" : new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }).format(at);
}
