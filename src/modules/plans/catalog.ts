import type { View } from "@/modules/core";

/** Os recursos que o admin do sistema liga, desliga e põe nos planos. A
 * mesma lista nasce na migração dos planos (`public.features`); o texto de
 * cada um vem do i18n (`feature.<chave>.title` / `.detail`). Os quatro do
 * Jev também são aplicados pelo núcleo (`src-tauri/crates/jayv-plans/src/features.rs`). */
export const FEATURES = [
  "organizations",
  "orgChat",
  "stats",
  "gateBoard",
  "liveFiles",
  "chatSearch",
  "projectNotes",
  "adaptiveRouting",
  "secondOpinion",
  "planFirst",
  "parallelTasks",
  "entryGate",
  "exitGate",
  "secretRedaction",
  "sensitiveFiles",
  "agentSessions",
  "contextCache",
  "answerRecall",
  "leanCode",
  "symbolIndex",
] as const;

export type FeatureKey = (typeof FEATURES)[number];

export const isFeature = (key: string): key is FeatureKey => (FEATURES as readonly string[]).includes(key);

/** A tela que só abre com o recurso. As outras abrem sempre. */
export const VIEW_FEATURE: Partial<Record<View, FeatureKey>> = {
  organizations: "organizations",
  organization: "organizations",
  stats: "stats",
  gate: "gateBoard",
};

/** O núcleo: está em todo plano, travado, e ninguém o desliga — nem o admin.
 * A mesma lista da migração `core_features` e do `CORE` do Rust: sem a lista do
 * servidor, ele continua travado. */
export const CORE_FEATURES: readonly FeatureKey[] = ["entryGate", "exitGate", "secretRedaction", "sensitiveFiles", "agentSessions", "contextCache", "adaptiveRouting"];

/** O que nunca vai para o contexto com `sensitiveFiles` travado. */
export const SENSITIVE_PATTERNS = [".env", ".env.*", "*.pem", "*.key", "*.p12", "*.pfx", "*.secret", ".ssh/**", "secrets/**"] as const;

/** O cache de contexto mais curto que o núcleo aceita, em segundos. */
export const MIN_CACHE_TTL = 300;
