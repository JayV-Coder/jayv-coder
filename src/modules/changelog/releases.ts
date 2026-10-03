/** O que cada versão publicada trouxe, da mais nova para a mais antiga. O
 * texto de cada item vem do i18n (`whatsNew.item.<id>.title` e `.detail`):
 * o inglês em `en.ts`, os outros idiomas na migração do JayV-Coder/supabase.
 *
 * Toda versão que sobe no `main` ganha a sua entrada aqui, no mesmo commit:
 * é o que a janela "Novidades" mostra depois da atualização. */

export type ChangeKind = "feature" | "fix";

export interface ReleaseItem {
  kind: ChangeKind;
  /** Pedaço da chave do i18n: `whatsNew.item.<id>.title` / `.detail`. */
  id: string;
}

export interface Release {
  version: string;
  /** Dia da publicação, `AAAA-MM-DD`. */
  date: string;
  items: ReleaseItem[];
}

export const RELEASES: Release[] = [
  {
    version: "0.35.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "projectNotes" },
      { kind: "feature", id: "gateLearning" },
      { kind: "feature", id: "routingFeedback" },
      { kind: "feature", id: "levelSuggestion" },
      { kind: "feature", id: "chatSearch" },
      { kind: "feature", id: "answerRecall" },
      { kind: "feature", id: "recipes" },
    ],
  },
  {
    version: "0.34.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "agentSessionResume" },
      { kind: "feature", id: "agentFileMap" },
      { kind: "feature", id: "parallelJev" },
      { kind: "feature", id: "promptCache" },
      { kind: "feature", id: "benchCommand" },
      { kind: "fix", id: "honestSavings" },
      { kind: "fix", id: "titleWithoutAgent" },
      { kind: "fix", id: "betterRetrieval" },
      { kind: "fix", id: "shortHistory" },
    ],
  },
  {
    version: "0.33.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "whatsNew" },
      { kind: "feature", id: "whatsNewHistory" },
    ],
  },
  {
    version: "0.32.1", date: "2026-10-02",
    items: [
      { kind: "fix", id: "fixedSidebar" },
    ],
  },
  {
    version: "0.32.0", date: "2026-10-02",
    items: [
      { kind: "feature", id: "orgChat" },
      { kind: "feature", id: "orgChatReach" },
      { kind: "feature", id: "orgChatPolicy" },
      { kind: "fix", id: "orgChatDashboard" },
    ],
  },
  {
    version: "0.31.0", date: "2026-10-02",
    items: [
      { kind: "feature", id: "twoFactor" },
      { kind: "feature", id: "twoFactorSignIn" },
      { kind: "feature", id: "twoFactorProtect" },
      { kind: "fix", id: "twoFactorPassword" },
    ],
  },
  {
    version: "0.29.0", date: "2026-10-02",
    items: [
      { kind: "feature", id: "lockdown" },
      { kind: "feature", id: "safeConnections" },
      { kind: "fix", id: "realtimeNotifications" },
    ],
  },
  {
    version: "0.27.0", date: "2026-10-02",
    items: [
      { kind: "feature", id: "orgStats" },
      { kind: "feature", id: "orgGate" },
    ],
  },
  {
    version: "0.26.0", date: "2026-10-02",
    items: [
      { kind: "feature", id: "usernameOnce" },
      { kind: "feature", id: "orgPolicyDefaults" },
      { kind: "fix", id: "agentIcons" },
      { kind: "fix", id: "agentExecutable" },
    ],
  },
  {
    version: "0.25.0", date: "2026-10-02",
    items: [
      { kind: "feature", id: "updateBanner" },
      { kind: "feature", id: "updateNotification" },
      { kind: "fix", id: "updateNoAutoInstall" },
    ],
  },
  {
    version: "0.24.0", date: "2026-10-02",
    items: [
      { kind: "feature", id: "orgFolderRemove" },
    ],
  },
  {
    version: "0.23.1", date: "2026-10-02",
    items: [
      { kind: "fix", id: "projectDeletion" },
    ],
  },
];
