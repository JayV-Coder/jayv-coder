import { create } from "zustand";
import { commands, onCore, type ModManifest, type ModsSnapshot } from "@/modules/core";
import { compareVersions } from "@/modules/changelog";
import { reportError } from "@/modules/feedback";
import { useEntitlements, type FeatureKey } from "@/modules/plans";

/** Os recursos opcionais que viraram mods (o `FEATURE_MODS` do núcleo e o
 * `engines.json` do repositório de mods). O núcleo, a conta e as organizações
 * ficam fora: não são mods. */
export const FEATURE_MODS = [
  "stats", "gateBoard", "liveFiles", "chatSearch", "projectNotes", "secondOpinion", "planFirst", "parallelTasks",
  "answerRecall", "leanCode", "symbolIndex", "mcp", "skills", "skillsHub", "conversationFind",
] as const satisfies readonly FeatureKey[];

export type FeatureMod = (typeof FEATURE_MODS)[number];

export const isFeatureMod = (id: string): id is FeatureMod => (FEATURE_MODS as readonly string[]).includes(id);

/** O que a tela sabe dos mods: o catálogo que desceu do repositório
 * `jayv-coder-mods` (nulo, nenhum desceu ainda), as escolhas gravadas e as do
 * rascunho da aba Mods, que só valem no Salvar, como o resto da tela. */
interface ModsState {
  loaded: boolean;
  catalog: ModManifest[] | null;
  fetchedAt: string | null;
  appVersion: string;
  /** As escolhas gravadas. */
  saved: Record<string, boolean>;
  /** As escolhas do rascunho. */
  states: Record<string, boolean>;
  saving: boolean;
}

export const useMods = create<ModsState>(() => ({ loaded: false, catalog: null, fetchedAt: null, appVersion: "0.0.0", saved: {}, states: {}, saving: false }));

const sorted = (states: Record<string, boolean>) => JSON.stringify(Object.keys(states).sort().map((key) => [key, states[key]]));

/** O rascunho difere do gravado. Uma escolha igual ao padrão do mod continua
 * sendo uma escolha: ela fica gravada. */
export const isModsDirty = (state: Pick<ModsState, "saved" | "states">) => sorted(state.saved) !== sorted(state.states);

/** O texto de um mod no idioma da tela: o do idioma, o da língua (`pt` para
 * `pt-BR`), o inglês ou o primeiro que houver. */
export function modText(texts: Record<string, string> | undefined, locale: string): string {
  if (!texts) return "";
  const language = locale.split("-")[0];
  const near = Object.keys(texts).find((key) => key.split("-")[0] === language);
  return texts[locale] ?? (near ? texts[near] : undefined) ?? texts.en ?? Object.values(texts)[0] ?? "";
}

/** O mod vale nesta versão do app. */
export const usable = (manifest: ModManifest, appVersion: string) => !manifest.minApp || compareVersions(manifest.minApp, appVersion) <= 0;

/** O mod do catálogo, do tipo e com o id pedidos. */
export const findMod = (catalog: ModManifest[] | null, kind: ModManifest["kind"], id: string) => catalog?.find((manifest) => manifest.kind === kind && manifest.id === id);

/** Os mods prontos que esta versão sabe instalar. */
export const templates = (catalog: ModManifest[] | null) => (catalog ?? []).filter((manifest) => manifest.kind === "template" && manifest.template !== undefined);

/** Onde um recurso está no catálogo: nenhum catálogo desceu (vale o app), o
 * mod saiu do catálogo, pede um app mais novo ou está lá. */
export type CatalogPresence = "unknown" | "absent" | "newer" | "present";

export function presence(state: Pick<ModsState, "catalog" | "appVersion">, kind: ModManifest["kind"], id: string): CatalogPresence {
  if (state.catalog === null) return "unknown";
  const manifest = findMod(state.catalog, kind, id);
  if (!manifest) return "absent";
  return usable(manifest, state.appVersion) ? "present" : "newer";
}

/** O recurso ligado no rascunho: a escolha de quem usa ou o padrão do mod
 * (ligado, sem catálogo). */
export function modOn(state: Pick<ModsState, "catalog" | "states">, id: string) {
  return state.states[id] ?? findMod(state.catalog, "feature", id)?.defaultOn ?? true;
}

function apply(snapshot: ModsSnapshot) {
  useMods.setState({ loaded: true, catalog: snapshot.catalog, fetchedAt: snapshot.fetchedAt, appVersion: snapshot.appVersion, saved: snapshot.states, states: snapshot.states });
  // O que os mods desligam vale como o que o plano não tem: some do menu, da
  // paleta e das abas.
  useEntitlements.setState({ disabled: new Set(snapshot.disabled) });
}

/** Relê o que o núcleo sabe. Com mudança pendente na aba Mods, o rascunho
 * fica: só o catálogo e o que está desligado mudam. */
export async function loadMods() {
  try {
    const snapshot = await commands.getMods();
    if (isModsDirty(useMods.getState())) {
      useMods.setState({ loaded: true, catalog: snapshot.catalog, fetchedAt: snapshot.fetchedAt, appVersion: snapshot.appVersion, saved: snapshot.states });
      useEntitlements.setState({ disabled: new Set(snapshot.disabled) });
    } else {
      apply(snapshot);
    }
  } catch (error) {
    console.error("mods", error);
  }
}

/** Liga ou desliga um recurso no rascunho. */
export function setModOn(id: FeatureMod, on: boolean) {
  useMods.setState((state) => ({ states: { ...state.states, [id]: on } }));
}

/** Grava o rascunho. Falso quando o núcleo recusou (o erro já foi mostrado). */
export async function saveModChanges(): Promise<boolean> {
  const state = useMods.getState();
  if (!isModsDirty(state)) return true;
  useMods.setState({ saving: true });
  try {
    apply(await commands.saveModStates(state.states));
    return true;
  } catch (error) {
    reportError(error);
    return false;
  } finally {
    useMods.setState({ saving: false });
  }
}

export function discardMods() {
  useMods.setState((state) => ({ states: state.saved }));
}

/** Os valores de partida: sem escolha nenhuma, vale o padrão de cada mod. */
export function restoreModDefaults() {
  useMods.setState({ states: {} });
}

/** Lê ao entrar, ao trocar de ambiente (cada um tem as suas escolhas) e ao
 * voltar para a janela (o catálogo novo desce na volta do sync). */
export function connectMods() {
  const focus = () => { if (document.visibilityState === "visible") void loadMods(); };
  window.addEventListener("focus", focus);
  const changed = onCore("environment-changed", () => {
    useMods.setState({ saved: {}, states: {} });
    void loadMods();
  });
  return () => {
    window.removeEventListener("focus", focus);
    void changed.then((unlisten) => unlisten());
  };
}

/** Ao sair da conta, nada fica: a próxima conta lê o dela. */
export function clearMods() {
  useMods.setState({ loaded: false, catalog: null, fetchedAt: null, saved: {}, states: {}, saving: false });
}

/** A página de mods do painel do site, no ambiente pedido: lá a pessoa cria,
 * edita e exclui os mods dela (`create` abre o formulário de criar). */
export function modsSitePath(environment: string, create = false) {
  const query = new URLSearchParams({ environment });
  if (create) query.set("create", "1");
  return `/mods?${query.toString()}`;
}
