// Gera o seed de `locales` e `translations` a partir dos arquivos de mensagens.
//
//   node scripts/seed-translations.ts > supabase/migrations/20260930120100_seed_locales.sql
//   node scripts/seed-translations.ts --check   # falha se algum idioma não traduz todas as chaves de `en`
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

type Message = string | Record<string, string>;

/** A mesma lista e ordem do seletor de idiomas. */
const LOCALES = [
  { id: "pt-BR", name: "Português (Brasil)", exported: "ptBR" },
  { id: "en", name: "English", exported: "en" },
  { id: "es", name: "Español", exported: "es" },
  { id: "zh-CN", name: "简体中文", exported: "zhCN" },
  { id: "hi", name: "हिन्दी", exported: "hi" },
  { id: "ar", name: "العربية", exported: "ar", rtl: true },
  { id: "fr", name: "Français", exported: "fr" },
  { id: "ru", name: "Русский", exported: "ru" },
  { id: "ja", name: "日本語", exported: "ja" },
  { id: "de", name: "Deutsch", exported: "de" },
];

const folder = resolve(import.meta.dirname, "../src/modules/i18n/messages");

async function messages(id: string, exported: string): Promise<Record<string, Message>> {
  const module = await import(pathToFileURL(`${folder}/${id}.ts`).href);
  return module[exported];
}

function literal(text: string): string {
  return `'${text.replaceAll("'", "''")}'`;
}

/** Dólar-citado para o JSON entrar sem escape nenhum. */
function json(value: Message): string {
  const text = JSON.stringify(value);
  if (text.includes("$json$")) throw new Error(`mensagem com $json$: ${text}`);
  return `$json$${text}$json$::jsonb`;
}

const loaded = await Promise.all(LOCALES.map(async (locale) => ({ ...locale, messages: await messages(locale.id, locale.exported) })));

if (process.argv.includes("--check")) {
  const keys = Object.keys(loaded.find((locale) => locale.id === "en")!.messages);
  const missing = loaded.flatMap((locale) => keys.filter((key) => !(key in locale.messages)).map((key) => `${locale.id}: ${key}`));
  if (missing.length) {
    console.error(`chaves sem tradução:\n${missing.join("\n")}`);
    process.exit(1);
  }
  console.log(`${loaded.length} idiomas, ${keys.length} chaves cada`);
} else {
  const lines = ["-- Gerado por scripts/seed-translations.ts. Não edite à mão.", ""];
  lines.push("insert into public.locales (id, name, rtl, position) values");
  lines.push(loaded.map((locale, position) => `  (${literal(locale.id)}, ${literal(locale.name)}, ${locale.rtl ? "true" : "false"}, ${position})`).join(",\n"));
  lines.push("on conflict (id) do update set name = excluded.name, rtl = excluded.rtl, position = excluded.position;", "");
  for (const locale of loaded) {
    lines.push("insert into public.translations (locale, key, value) values");
    lines.push(Object.entries(locale.messages).map(([key, value]) => `  (${literal(locale.id)}, ${literal(key)}, ${json(value)})`).join(",\n"));
    lines.push("on conflict (locale, key) do update set value = excluded.value;", "");
  }
  console.log(lines.join("\n"));
}
