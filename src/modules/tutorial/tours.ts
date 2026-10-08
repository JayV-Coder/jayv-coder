import type { View } from "@/modules/core";

/** Um passo do tutorial: a funcionalidade que ele apresenta (o texto é o da
 * documentação, `docs/manual/features/<feature>.json`, traduzido pelas chaves
 * `docs.<feature>.*`) e, se houver, o elemento da tela que ele destaca
 * (`data-tour="<target>"`). Sem alvo, ou com o alvo fora da tela, o passo abre
 * no centro. */
export interface TourStep {
  feature: string;
  target?: string;
}

export interface Tour {
  id: string;
  /** A tela em que o tutorial abre sozinho na primeira visita. */
  view: View;
  steps: TourStep[];
}

/** Todo recurso da documentação aparece em pelo menos um tutorial (o teste
 * confere): funcionalidade nova ganha o seu passo aqui, no mesmo commit. */
export const TOURS: Tour[] = [
  {
    id: "projects", view: "projects",
    steps: [
      { feature: "agents" },
      { feature: "commandPalette" },
      { feature: "organizations", target: "nav-organizations" },
      { feature: "environments", target: "environment-switch" },
      { feature: "stats", target: "nav-stats" },
      { feature: "updates" },
      { feature: "tutorials", target: "help-tutorial" },
    ],
  },
  {
    id: "chat", view: "chat",
    steps: [
      { feature: "workModes", target: "composer-mode" },
      { feature: "stopRequest", target: "composer-send" },
      { feature: "progressPanel" },
      { feature: "parallelRequests" },
      { feature: "chatSearch" },
      { feature: "conversationFind" },
      { feature: "consumptionGuard" },
      { feature: "answerRecall" },
      { feature: "liveFiles", target: "header-live" },
      { feature: "projectNotes" },
      { feature: "orgChat" },
      { feature: "commandPermissions", target: "composer-grants" },
    ],
  },
  {
    id: "gate", view: "gate",
    steps: [
      { feature: "entryGate" },
      { feature: "exitGate" },
      { feature: "gateBoard", target: "nav-gate" },
      { feature: "answerEvidence" },
    ],
  },
  {
    id: "settings", view: "settings",
    steps: [
      { feature: "agents", target: "settings-tabs" },
      { feature: "kiloCode" },
      { feature: "gatewayProviders" },
      { feature: "adaptiveRouting" },
      { feature: "agentSessions" },
      { feature: "secondOpinion" },
      { feature: "planFirst" },
      { feature: "parallelTasks" },
      { feature: "mcp" },
      { feature: "skills" },
      { feature: "skillsHub" },
      { feature: "contextCache" },
      { feature: "leanCode" },
      { feature: "symbolIndex" },
      { feature: "secretRedaction" },
      { feature: "sensitiveFiles" },
      { feature: "cli" },
    ],
  },
  {
    id: "organizations", view: "organizations",
    steps: [
      { feature: "organizations" },
      { feature: "orgRepositories" },
      { feature: "orgExtensions" },
    ],
  },
  {
    id: "profile", view: "profile",
    steps: [
      { feature: "profilePhoto" },
      { feature: "twoFactor" },
    ],
  },
];

export const tourOf = (id: string): Tour | undefined => TOURS.find((tour) => tour.id === id);

/** O tutorial da tela aberta, quando há um. */
export const tourForView = (view: View): Tour | undefined => TOURS.find((tour) => tour.view === view);
