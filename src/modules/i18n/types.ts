/** Uma mensagem é texto com `{marcas}` para os valores, ou uma forma por
 * categoria de plural — o árabe e o russo pedem mais do que singular e plural,
 * e é o `Intl.PluralRules` do idioma que escolhe qual. */
export type Plural = { zero?: string; one?: string; two?: string; few?: string; many?: string; other: string };
export type Message = string | Plural;

import type { en } from "./messages/en";

export type Key = keyof typeof en;
/** O que chega do Supabase para um idioma: pode faltar chave — a nova, que
 * ainda não foi traduzida —, e aí vale o inglês. */
export type Messages = Partial<Record<Key, Message>>;
