/** Uma mensagem é texto com `{marcas}` para os valores, ou uma forma por
 * categoria de plural — o árabe e o russo pedem mais do que singular e plural,
 * e é o `Intl.PluralRules` do idioma que escolhe qual. */
export type Plural = { zero?: string; one?: string; two?: string; few?: string; many?: string; other: string };
export type Message = string | Plural;

import type { ptBR } from "./messages/pt-BR";

export type Key = keyof typeof ptBR;
/** Todo idioma traduz todas as chaves: faltar uma é erro de tipo, não texto
 * em português aparecendo no meio de outra língua. */
export type Messages = Record<Key, Message>;
