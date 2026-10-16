import { create } from "zustand";
import { commands, onCore, type InstalledMod, type InstalledMods, type ModAgent, type ModManifest, type ModSkill, type ModsSnapshot } from "@/modules/core";
import { compareVersions } from "@/modules/changelog";
import { reportError } from "@/modules/feedback";
import { useEntitlements, type FeatureKey } from "@/modules/plans";

/** Os recursos opcionais que um mod pode ligar (o `FEATURE_MODS` do núcleo e o
 * `engines.json` do repositório de mods). O núcleo, a conta e as organizações
 * ficam fora: não são de mod nenhum. */
export const FEATURE_MODS = [
  "stats", "gateBoard", "liveFiles", "chatSearch", "projectNotes", "secondOpinion", "planFirst", "parallelTasks",
  "answerRecall", "leanCode", "symbolIndex", "mcp", "skills", "skillsHub", "conversationFind",
] as const satisfies readonly FeatureKey[];

export type FeatureMod = (typeof FEATURE_MODS)[number];

export const isFeatureMod = (id: string): id is FeatureMod => (FEATURE_MODS as readonly string[]).includes(id);

/** Os pontos de extensão que esta tela sabe mostrar. O núcleo diz quais ele
 * aplica (`points`); um ponto que esta tela não conhece aparece como "outro". */
export const KNOWN_POINTS = ["llmEngines", "features", "agents", "skills"] as const;
export type KnownPoint = (typeof KNOWN_POINTS)[number];

const EMPTY: InstalledMods = { mods: {}, removed: [] };

/** O que a tela sabe dos mods: o catálogo que desceu do repositório
 * `jayv-coder-mods` (nulo, nenhum desceu ainda), o estado gravado do ambiente
 * e o rascunho da aba Mods — instalar, ligar, desligar e desinstalar só valem
 * no Salvar, como o resto da tela. */
interface ModsState {
  loaded: boolean;
  catalog: ModManifest[] | null;
  fetchedAt: string | null;
  appVersion: string;
  /** Os pontos de extensão que o núcleo aplica. */
  points: string[];
  /** Os mods que valem agora, no gravado. */
  active: string[];
  /** O estado gravado. */
  saved: InstalledMods;
  /** O rascunho. */
  draft: InstalledMods;
  saving: boolean;
  /** A aba da lista de instalados: os que a pessoa instalou ou os essenciais. */
  installedTab: InstalledTab;
}

/** As abas da lista de instalados: os mods que a pessoa instalou (motores,
 * agentes, skills) e os essenciais, que o app mantém instalados e ligados. */
export const INSTALLED_TABS = ["installed", "essentials"] as const;
export type InstalledTab = (typeof INSTALLED_TABS)[number];

export const useMods = create<ModsState>(() => ({
  loaded: false, catalog: null, fetchedAt: null, appVersion: "0.0.0", points: [...KNOWN_POINTS], active: [], saved: EMPTY, draft: EMPTY, saving: false, installedTab: "installed",
}));

/** Troca a aba da lista de instalados (o clique na aba e o Ctrl+K). */
export function setInstalledTab(tab: InstalledTab) {
  useMods.setState({ installedTab: tab });
}

/** Os instalados de cada aba, na ordem em que chegam: o essencial vai para a
 * dele; o resto, inclusive o que saiu do catálogo, fica em Instalados. */
export function splitInstalled(ids: string[], catalog: ModManifest[] | null): Record<InstalledTab, string[]> {
  return {
    installed: ids.filter((id) => !isRequired(catalog, id)),
    essentials: ids.filter((id) => isRequired(catalog, id)),
  };
}

/** O que importa comparar: quem está instalado e se está ligado. */
const shape = (state: InstalledMods) => JSON.stringify(Object.keys(state.mods).sort().map((id) => [id, state.mods[id].enabled]));

export const isModsDirty = (state: Pick<ModsState, "saved" | "draft">) => shape(state.saved) !== shape(state.draft);

/** O texto de um mod no idioma da tela: o do idioma, o da língua (`pt` para
 * `pt-BR`), o inglês ou o primeiro que houver. */
export function modText(texts: Record<string, string> | undefined, locale: string): string {
  if (!texts) return "";
  const language = locale.split("-")[0];
  const near = Object.keys(texts).find((key) => key.split("-")[0] === language);
  return texts[locale] ?? (near ? texts[near] : undefined) ?? texts.en ?? Object.values(texts)[0] ?? "";
}

/** O mod do catálogo com o id pedido. */
export const findMod = (catalog: ModManifest[] | null, id: string) => catalog?.find((manifest) => manifest.id === id);

/** Os pontos que o mod usa. */
export const pointsOf = (manifest: ModManifest) => Object.keys(manifest.contributes ?? {});

/** A versão do app basta, mas o mod usa um ponto que este app não aplica. */
export const unknownPoints = (manifest: ModManifest, points: readonly string[]) => pointsOf(manifest).filter((point) => !points.includes(point));

/** O mod roda nesta versão: o `minApp` cabe e o app aplica todo ponto dele. */
export const usable = (manifest: ModManifest, appVersion: string, points: readonly string[] = KNOWN_POINTS) =>
  (!manifest.minApp || compareVersions(manifest.minApp, appVersion) <= 0) && unknownPoints(manifest, points).length === 0;

const strings = (value: unknown): string[] => (Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : []);

/** Os motores de LLM que o mod liga. */
export const modEngines = (manifest: ModManifest) => strings(manifest.contributes?.llmEngines);
/** Os recursos que o mod liga. */
export const modFeatures = (manifest: ModManifest) => strings(manifest.contributes?.features).filter(isFeatureMod);
/** Os agentes que o mod traz. */
export const modAgents = (manifest: ModManifest): ModAgent[] => (Array.isArray(manifest.contributes?.agents) ? manifest.contributes.agents : []);

/** As skills que o mod traz. */
export const modSkills = (manifest: ModManifest): ModSkill[] => (Array.isArray(manifest.contributes?.skills) ? manifest.contributes.skills : []);

/** O id do agente que a entrada `index` do mod vira: o mesmo do núcleo e do
 * site. */
export const modAgentId = (modId: string, index: number) => (index === 0 ? `mod-${modId}` : `mod-${modId}-${index + 1}`) as `mod-${string}`;

/** Os agentes (ids) que o mod traz: os motores de LLM e os agentes dele. */
export const modAgentIds = (manifest: ModManifest): string[] => [...modEngines(manifest), ...modAgents(manifest).map((_agent, index) => modAgentId(manifest.id, index))];

/** O mod do catálogo que traz o agente (`mod-<id>`, `mod-<id>-2`…), para a
 * aba e o ícone dele; nulo no agente que a pessoa criou. */
export function modOfAgent(catalog: ModManifest[] | null, agentId: string): ModManifest | null {
  return (catalog ?? []).find((manifest) => modAgents(manifest).some((_agent, index) => modAgentId(manifest.id, index) === agentId)) ?? null;
}

/** Os agentes que pertencem aos mods instalados no estado: não se apagam
 * pela aba deles, saem pelo desinstalar do mod. */
export function ownedAgents(catalog: ModManifest[] | null, state: InstalledMods): Set<string> {
  return new Set((catalog ?? []).filter((manifest) => state.mods[manifest.id]).flatMap((manifest) => modAgents(manifest).map((_agent, index) => modAgentId(manifest.id, index))));
}

/** Essencial: o app precisa dele, e ele não se desliga nem se desinstala. */
export const isRequired = (catalog: ModManifest[] | null, id: string) => findMod(catalog, id)?.required === true;

/** Instalado no rascunho. */
export const isInstalled = (state: Pick<ModsState, "draft">, id: string) => id in state.draft.mods;

/** Ligado no rascunho. */
export const isOn = (state: Pick<ModsState, "draft">, id: string) => state.draft.mods[id]?.enabled ?? false;

/** As dependências do mod que não estão instaladas e ligadas no rascunho. */
export function missingRequirements(state: Pick<ModsState, "draft">, manifest: ModManifest): string[] {
  return (manifest.requires ?? []).filter((other) => !isOn(state, other));
}

/** Os mods instalados que precisam deste (desinstalar ou desligar este os
 * para). */
export function dependents(state: Pick<ModsState, "draft" | "catalog">, id: string): ModManifest[] {
  return (state.catalog ?? []).filter((manifest) => isInstalled(state, manifest.id) && (manifest.requires ?? []).includes(id));
}

/** O estado do rascunho com o mod e as dependências dele instalados e
 * ligados. */
export function withInstalled(state: InstalledMods, catalog: ModManifest[] | null, id: string): InstalledMods {
  const mods: Record<string, InstalledMod> = { ...state.mods };
  const removed = new Set(state.removed);
  const queue = [id];
  while (queue.length > 0) {
    const next = queue.pop()!;
    if (!mods[next]) mods[next] = { enabled: true, version: findMod(catalog, next)?.version ?? null };
    else if (next === id) mods[next] = { ...mods[next], enabled: true };
    removed.delete(next);
    for (const other of findMod(catalog, next)?.requires ?? []) if (!mods[other] && findMod(catalog, other)) queue.push(other);
  }
  return { mods, removed: [...removed] };
}

/** Os valores de partida: os mods de fábrica instalados e todo instalado
 * ligado. Nada se desinstala. */
export function defaultsOf(state: InstalledMods, catalog: ModManifest[] | null): InstalledMods {
  let next: InstalledMods = { mods: Object.fromEntries(Object.entries(state.mods).map(([id, mod]) => [id, { ...mod, enabled: true }])), removed: [] };
  for (const manifest of catalog ?? []) if (manifest.preinstalled) next = withInstalled(next, catalog, manifest.id);
  return next;
}

/** Como o mod está, para o rótulo da linha: ativo, desligado, esperando um
 * app mais novo, sem uma dependência ou fora do catálogo. */
export type ModStatus = "active" | "off" | "needsApp" | "missing" | "absent";

export function statusOf(state: Pick<ModsState, "draft" | "catalog" | "appVersion" | "points">, id: string): ModStatus {
  const manifest = findMod(state.catalog, id);
  if (state.catalog !== null && !manifest) return "absent";
  if (!isOn(state, id)) return "off";
  if (manifest && !usable(manifest, state.appVersion, state.points)) return "needsApp";
  if (manifest && missingRequirements(state, manifest).length > 0) return "missing";
  return "active";
}

/** O rascunho levado para cima do gravado novo: o que a pessoa mudou na aba
 * (instalou, desinstalou, ligou, desligou) fica, e o resto vem do gravado
 * novo. O Salvar grava o estado inteiro: sem isto, o mod que outra máquina ou
 * o site instalou enquanto a aba tinha mudança pendente era desinstalado no
 * Salvar (e o de fábrica não voltava mais). */
export function rebaseDraft(before: InstalledMods, draft: InstalledMods, saved: InstalledMods): InstalledMods {
  const mods: Record<string, InstalledMod> = { ...saved.mods };
  for (const id of new Set([...Object.keys(before.mods), ...Object.keys(draft.mods)])) {
    const was = before.mods[id];
    const wanted = draft.mods[id];
    if (!wanted) { if (was) delete mods[id]; continue; }
    if (!was || was.enabled !== wanted.enabled) mods[id] = mods[id] ? { ...mods[id], enabled: wanted.enabled } : wanted;
  }
  return { mods, removed: saved.removed.filter((id) => !(id in mods)) };
}

function apply(snapshot: ModsSnapshot, keepDraft: boolean) {
  useMods.setState((state) => ({
    loaded: true, catalog: snapshot.catalog, fetchedAt: snapshot.fetchedAt, appVersion: snapshot.appVersion, points: snapshot.points,
    active: snapshot.active, saved: snapshot.state, draft: keepDraft ? rebaseDraft(state.saved, state.draft, snapshot.state) : snapshot.state,
  }));
  // O que os mods desligam vale como o que o plano não tem: some do menu, da
  // paleta e das abas.
  useEntitlements.setState({ disabled: new Set(snapshot.disabled) });
}

/** Relê o que o núcleo sabe. Com mudança pendente na aba Mods, o rascunho
 * fica: só o catálogo, o gravado e o que está desligado mudam. */
export async function loadMods() {
  try {
    const snapshot = await commands.getMods();
    apply(snapshot, isModsDirty(useMods.getState()));
  } catch (error) {
    console.error("mods", error);
  }
}

/** Instala o mod no rascunho, ligado, com as dependências. */
export function installMod(id: string) {
  useMods.setState((state) => ({ draft: withInstalled(state.draft, state.catalog, id) }));
}

/** Tira o mod do rascunho. O núcleo, ao salvar, roda o desinstalar dele e
 * guarda o de fábrica entre os removidos, para não voltar sozinho. */
export function uninstallMod(id: string) {
  useMods.setState((state) => {
    if (isRequired(state.catalog, id)) return state;
    const { [id]: _gone, ...mods } = state.draft.mods;
    return { draft: { ...state.draft, mods } };
  });
}

/** Liga ou desliga o mod no rascunho. Ligar puxa as dependências. */
export function setModEnabled(id: string, enabled: boolean) {
  useMods.setState((state) => {
    if (enabled) return { draft: withInstalled(state.draft, state.catalog, id) };
    if (isRequired(state.catalog, id)) return state;
    const current = state.draft.mods[id];
    return current ? { draft: { ...state.draft, mods: { ...state.draft.mods, [id]: { ...current, enabled: false } } } } : state;
  });
}

/** Grava o rascunho. Falso quando o núcleo recusou (o erro já foi mostrado). */
export async function saveModChanges(): Promise<boolean> {
  const state = useMods.getState();
  if (!isModsDirty(state)) return true;
  useMods.setState({ saving: true });
  try {
    apply(await commands.saveMods(state.draft), false);
    return true;
  } catch (error) {
    reportError(error);
    return false;
  } finally {
    useMods.setState({ saving: false });
  }
}

export function discardMods() {
  useMods.setState((state) => ({ draft: state.saved }));
}

export function restoreModDefaults() {
  useMods.setState((state) => ({ draft: defaultsOf(state.draft, state.catalog) }));
}

/** Lê ao entrar, ao trocar de ambiente (cada um tem os seus mods), ao voltar
 * para a janela e quando a instalação sozinha mudou alguma coisa (no login, ou
 * o que outra máquina ou o site decidiu). */
export function connectMods() {
  const focus = () => { if (document.visibilityState === "visible") void loadMods(); };
  window.addEventListener("focus", focus);
  const changed = onCore("environment-changed", () => {
    useMods.setState({ saved: EMPTY, draft: EMPTY });
    void loadMods();
  });
  const installed = onCore("mods-changed", () => void loadMods());
  // Ligar ou desligar um mod em outra máquina (ou no site) não instala nem
  // desinstala nada aqui: só desce o estado novo, e é por este aviso que a aba
  // e o que fica escondido acompanham.
  const synced = onCore("workspace-synced", () => void loadMods());
  return () => {
    window.removeEventListener("focus", focus);
    void changed.then((unlisten) => unlisten());
    void installed.then((unlisten) => unlisten());
    void synced.then((unlisten) => unlisten());
  };
}

/** Ao sair da conta, nada fica: a próxima conta lê o dela. */
export function clearMods() {
  useMods.setState({ loaded: false, catalog: null, fetchedAt: null, active: [], saved: EMPTY, draft: EMPTY, saving: false, installedTab: "installed" });
}

/** A página pública do mod no site, no idioma da tela: o que ele faz, como
 * habilitar e como usar, com prints. Sem `id`, o catálogo com a busca. */
export const modPagePath = (locale: string, id?: string) => `/${locale}/mods${id ? `/${encodeURIComponent(id)}` : ""}`;

/** A página de mods do painel do site, no ambiente pedido: lá a pessoa
 * instala, liga e desliga os mods do catálogo e cria, edita e exclui os dela
 * (`create` abre o formulário de criar). */
export function modsSitePath(environment: string, create = false) {
  const query = new URLSearchParams({ environment });
  if (create) query.set("create", "1");
  return `/mods?${query.toString()}`;
}
