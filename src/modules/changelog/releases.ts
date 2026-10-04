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
  /** O inglês do item, quando ele vem de uma versão que este app ainda não
   * conhece (as notas do release novo): vale enquanto a chave não existe. */
  title?: string | null;
  detail?: string | null;
}

export interface Release {
  version: string;
  /** Dia da publicação, `AAAA-MM-DD`. */
  date: string;
  items: ReleaseItem[];
}

export const RELEASES: Release[] = [
  {
    version: "0.53.0", date: "2026-10-04",
    items: [
      { kind: "feature", id: "orgAdminOnSite" },
      { kind: "feature", id: "orgSettingsOnSite" },
      { kind: "feature", id: "accountOnSite" },
      { kind: "feature", id: "adminOnSite" },
    ],
  },
  {
    version: "0.52.4", date: "2026-10-04",
    items: [
      { kind: "fix", id: "planSessionInBuild" },
    ],
  },
  {
    version: "0.52.3", date: "2026-10-04",
    items: [
      { kind: "fix", id: "featureStatsPlans" },
    ],
  },
  {
    version: "0.52.2", date: "2026-10-04",
    items: [
      { kind: "fix", id: "featureSystem" },
    ],
  },
  {
    version: "0.52.1", date: "2026-10-04",
    items: [
      { kind: "fix", id: "coreOrchestration" },
    ],
  },
  {
    version: "0.52.0", date: "2026-10-04",
    items: [
      { kind: "feature", id: "agentMechanisms" },
      { kind: "feature", id: "thoughtsCollapsed" },
      { kind: "fix", id: "unsavedAbove" },
    ],
  },
  {
    version: "0.51.7", date: "2026-10-04",
    items: [
      { kind: "fix", id: "coreWorkspaceLive" },
    ],
  },
  {
    version: "0.51.6", date: "2026-10-04",
    items: [
      { kind: "fix", id: "coreCodeMemory" },
    ],
  },
  {
    version: "0.51.5", date: "2026-10-04",
    items: [
      { kind: "fix", id: "coreJevPlansOrgs" },
    ],
  },
  {
    version: "0.51.4", date: "2026-10-04",
    items: [
      { kind: "fix", id: "interruptedNotResumed" },
      { kind: "fix", id: "indexAfterBuild" },
      { kind: "fix", id: "settingsWhileWorking" },
      { kind: "fix", id: "policyAcrossOrgs" },
      { kind: "fix", id: "titlesWithCli" },
    ],
  },
  {
    version: "0.51.3", date: "2026-10-04",
    items: [
      { kind: "fix", id: "coreCloudAgents" },
    ],
  },
  {
    version: "0.51.2", date: "2026-10-04",
    items: [
      { kind: "fix", id: "coreCrates" },
    ],
  },
  {
    version: "0.51.1", date: "2026-10-04",
    items: [
      { kind: "fix", id: "coreLayers" },
    ],
  },
  {
    version: "0.51.0", date: "2026-10-04",
    items: [
      { kind: "feature", id: "plans" },
      { kind: "feature", id: "adminFeatures" },
    ],
  },
  {
    version: "0.50.1", date: "2026-10-04",
    items: [
      { kind: "fix", id: "queueUnstuck" },
      { kind: "fix", id: "editsKept" },
      { kind: "fix", id: "safetyFixes" },
    ],
  },
  {
    version: "0.50.0", date: "2026-10-04",
    items: [
      { kind: "feature", id: "parallelTasks" },
    ],
  },
  {
    version: "0.49.0", date: "2026-10-04",
    items: [
      { kind: "feature", id: "planFirst" },
    ],
  },
  {
    version: "0.48.0", date: "2026-10-04",
    items: [
      { kind: "feature", id: "secondOpinion" },
    ],
  },
  {
    version: "0.47.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "agentOrder" },
      { kind: "fix", id: "modelFamilies" },
    ],
  },
  {
    version: "0.46.0", date: "2026-10-03",
    items: [
      { kind: "fix", id: "fairRouting" },
      { kind: "feature", id: "agentFallback" },
    ],
  },
  {
    version: "0.45.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "trayBackground" },
      { kind: "fix", id: "updatesInBackground" },
      { kind: "fix", id: "liveFixes" },
      { kind: "fix", id: "chatFilterWidth" },
    ],
  },
  {
    version: "0.44.3", date: "2026-10-03",
    items: [
      { kind: "fix", id: "buildWrites" },
    ],
  },
  {
    version: "0.44.2", date: "2026-10-03",
    items: [
      { kind: "fix", id: "upcomingNotes" },
      { kind: "fix", id: "chatFits" },
    ],
  },
  {
    version: "0.44.1", date: "2026-10-03",
    items: [
      { kind: "fix", id: "answerNotBlocked" },
    ],
  },
  {
    version: "0.44.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "readableCode" },
    ],
  },
  {
    version: "0.43.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "workMode" },
    ],
  },
  {
    version: "0.42.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "chatBubbles" },
      { kind: "feature", id: "recentSteps" },
      { kind: "fix", id: "readableButtons" },
    ],
  },
  {
    version: "0.41.1", date: "2026-10-03",
    items: [
      { kind: "fix", id: "translationsComplete" },
      { kind: "fix", id: "updateEvery15s" },
      { kind: "fix", id: "translatedSteps" },
    ],
  },
  {
    version: "0.41.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "liveFiles" },
    ],
  },
  {
    version: "0.40.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "chatRepositories" },
      { kind: "feature", id: "updateInterval" },
      { kind: "fix", id: "missingTranslations" },
    ],
  },
  {
    version: "0.39.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "systemHealth" },
      { kind: "feature", id: "systemDiagnostics" },
    ],
  },
  {
    version: "0.38.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "repoAgent" },
      { kind: "feature", id: "projectFilters" },
      { kind: "feature", id: "updateChoice" },
      { kind: "fix", id: "orgPageTidy" },
    ],
  },
  {
    version: "0.37.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "terminalLook" },
      { kind: "feature", id: "commandPalette" },
      { kind: "feature", id: "statusBar" },
    ],
  },
  {
    version: "0.36.0", date: "2026-10-03",
    items: [
      { kind: "feature", id: "leanCode" },
      { kind: "feature", id: "symbolIndex" },
      { kind: "feature", id: "projectMap" },
      { kind: "feature", id: "symbolTools" },
    ],
  },
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
