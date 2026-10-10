import { create } from "zustand";
import { useShallow } from "zustand/react/shallow";
import {
  bus, commands, onCore, type AgentId, type AgentModel, type AgentOptions, type AgentProbe, type AgentSettings, type BuiltInAgentId, type CustomAgentId, type CustomModOptions,
  type GatewayCheck, type KnownModel, CUSTOM_MOD_PREFIX, editsProject, isApiAgent, isCustomMod, isGateway,
  type CoreSettings, type CoreSnapshot, type Expertise, type SettingsSnapshot,
} from "@/modules/core";
import { notify, reportError } from "@/modules/feedback";
import { setLocale, syncLanguage, t, useI18n, type Key, type Locale } from "@/modules/i18n";
import { discardMcp, isMcpDirty, loadMcp, saveMcpChanges, useMcp } from "@/modules/mcp";
import { discardSkills, isSkillsDirty, loadSkills, saveSkillChanges, useSkills } from "@/modules/skills";
import { setThemePreference, useTheme, type ThemePreference } from "@/modules/theme";
import { setAutoTours, useTutorial } from "@/modules/tutorial";
import { setInstallOnLaunch, useUpdate } from "@/modules/updates";
import { lineProblem, modId, splitLine } from "./mods";

/** Os mods que vêm com o app, na ordem das abas. Os criados vêm depois, pelo
 * nome (`agentIds`). */
export const AGENTS: BuiltInAgentId[] = ["claude", "codex", "copilot", "cursor", "kilo", "openrouter", "litellm"];
export const AGENT_LABELS: Record<BuiltInAgentId, string> = {
  claude: "Claude Code", codex: "Codex", copilot: "GitHub Copilot", cursor: "Cursor", kilo: "Kilo Code", openrouter: "OpenRouter", litellm: "LiteLLM",
};
/** Os mods do app que são um programa na máquina: só neles há o que conferir
 * (instalado, versão, login). Os gateways de API têm endereço e chave. Com as
 * configurações carregadas, `cliAgentIds` soma os mods criados. */
export const CLI_AGENTS: AgentId[] = AGENTS.filter((id) => !isGateway(id));

/** O modelo como está na aba: `uid` segura a linha enquanto o identificador
 * muda. */
export interface ModelDraft extends AgentModel { uid: string }

/** A verificação do executável: `null` enquanto ninguém conferiu. */
export type ProbeState = AgentProbe | "checking" | null;

interface SettingsState {
  loaded: boolean;
  agents: AgentSettings[];
  models: ModelDraft[];
  catalog: Record<AgentId, KnownModel[]>;
  timeoutRange: [number, number];
  contextRange: [number, number];
  probes: Record<AgentId, ProbeState>;
  /** A conferência de cada gateway de API (endereço e chave gravados). */
  gateways: Partial<Record<AgentId, GatewayCheck | "checking">>;
  /** O que foi gravado por último, para saber se há alteração pendente. */
  saved: string;
  saving: boolean;
  /** O agente cuja lista de modelos está sendo lida do CLI. */
  refreshing: AgentId | null;
  /** As abas do Jev e do app: o rascunho, o que está gravado e o resto do
   * retrato (padrões, limites, números da portaria). */
  core: CoreSettings | null;
  coreSnapshot: CoreSnapshot | null;
  savedCore: string;
  /** O que a aba App muda fora do núcleo (idioma, tema, tutoriais, atualizar
   * ao abrir), esperando o Salvar: só as escolhas que diferem do que vale. */
  prefs: Partial<AppPrefs>;
}

/** As escolhas da aba App que moram na tela, não no núcleo. */
export interface AppPrefs {
  locale: Locale;
  theme: ThemePreference;
  tutorialAuto: boolean;
  installOnLaunch: boolean;
}

/** O que vale agora em cada escolha da aba App. */
export function livePrefs(): AppPrefs {
  return {
    locale: useI18n.getState().locale,
    theme: useTheme.getState().preference,
    tutorialAuto: useTutorial.getState().auto,
    installOnLaunch: useUpdate.getState().installOnLaunch,
  };
}

/** Os padrões da aba App que o Restaurar padrões devolve (o idioma fica). */
const DEFAULT_PREFS: Omit<AppPrefs, "locale"> = { theme: "system", tutorialAuto: true, installOnLaunch: true };

let next = 0;
const uid = () => `model-${++next}`;
const noProbes = (): Record<AgentId, ProbeState> => ({ claude: null, codex: null, copilot: null, cursor: null, kilo: null, openrouter: null, litellm: null });

/** O nome de um mod onde quer que ele apareça (o chat, o uso, a portaria): o
 * do app, o que a pessoa deu ao criado ou, sem as configurações carregadas, o
 * id do criado sem o prefixo. Outro texto (um provedor antigo) fica como está. */
export function agentLabel(id: string, agents: AgentSettings[] = useSettings.getState().agents): string {
  if (id in AGENT_LABELS) return AGENT_LABELS[id as BuiltInAgentId];
  if (!isCustomMod(id)) return id;
  const name = (agents.find((agent) => agent.id === id)?.options as Partial<CustomModOptions> | undefined)?.name?.trim();
  return name || id.slice(CUSTOM_MOD_PREFIX.length);
}

/** Os mods na ordem das abas: os do app e depois os criados, pelo nome. */
export function agentIds(agents: AgentSettings[]): AgentId[] {
  const customs = agents.filter((agent) => isCustomMod(agent.id)).map((agent) => agent.id)
    .sort((a, b) => agentLabel(a, agents).localeCompare(agentLabel(b, agents)) || a.localeCompare(b));
  return [...AGENTS, ...customs];
}

/** Os mods que são um programa na máquina: os do app e os criados de linha
 * de comando. */
export function cliAgentIds(agents: AgentSettings[]): AgentId[] {
  return agentIds(agents).filter((id) => {
    const agent = agents.find((item) => item.id === id);
    return agent ? !isApiAgent(agent) : !isGateway(id);
  });
}

export const useSettings = create<SettingsState>(() => ({
  loaded: false, agents: [], models: [], catalog: { claude: [], codex: [], copilot: [], cursor: [], kilo: [], openrouter: [], litellm: [] }, timeoutRange: [30, 3600],
  contextRange: [8000, 2000000], probes: noProbes(), gateways: {}, saved: "", saving: false, refreshing: null,
  core: null, coreSnapshot: null, savedCore: "", prefs: {},
}));

const payload = ({ agents, models }: Pick<SettingsState, "agents" | "models">) =>
  ({ agents, models: models.map(({ uid: _, ...model }) => model) });

export const isAgentsDirty = (state: SettingsState) => state.loaded && JSON.stringify(payload(state)) !== state.saved;
export const isCoreDirty = (state: SettingsState) => state.core !== null && JSON.stringify(state.core) !== state.savedCore;
export const isDirty = (state: SettingsState) => isAgentsDirty(state) || isCoreDirty(state);

/** As escolhas da aba App que mudam algo ao salvar. */
function changedPrefs(prefs: Partial<AppPrefs>, live: AppPrefs): Partial<AppPrefs> {
  return Object.fromEntries(Object.entries(prefs).filter(([key, value]) => live[key as keyof AppPrefs] !== value)) as Partial<AppPrefs>;
}

/** Há alguma mudança pendente na tela de configurações, de qualquer aba
 * (agentes, Jev, app, MCP, skills). */
export function useSettingsDirty() {
  const settings = useSettings(isDirty);
  const prefs = useSettings((state) => state.prefs);
  const live = useAppPrefsLive();
  const mcp = useMcp(isMcpDirty);
  const skills = useSkills(isSkillsDirty);
  return settings || mcp || skills || Object.keys(changedPrefs(prefs, live)).length > 0;
}

function useAppPrefsLive(): AppPrefs {
  const locale = useI18n((state) => state.locale);
  const theme = useTheme((state) => state.preference);
  const tutorialAuto = useTutorial((state) => state.auto);
  const installOnLaunch = useUpdate((state) => state.installOnLaunch);
  return { locale, theme, tutorialAuto, installOnLaunch };
}

/** As escolhas da aba App como a tela as mostra: a pendente ou a que vale. */
export function useAppPrefs(): AppPrefs {
  const live = useAppPrefsLive();
  const prefs = useSettings((state) => state.prefs);
  return { ...live, ...prefs };
}

/** Muda uma escolha da aba App sem aplicar: vale no Salvar. */
export function updatePrefs(patch: Partial<AppPrefs>) {
  useSettings.setState((state) => ({ prefs: changedPrefs({ ...state.prefs, ...patch }, livePrefs()) }));
}

function applyCore(snapshot: CoreSnapshot) {
  useSettings.setState({ core: snapshot.settings, coreSnapshot: snapshot, savedCore: JSON.stringify(snapshot.settings) });
}

/** Muda um campo das abas do Jev e do app. */
export function updateCore(patch: Partial<CoreSettings>) {
  useSettings.setState((state) => (state.core ? { core: { ...state.core, ...patch } } : state));
}

/** Volta as abas do Jev e do app aos valores de partida (sem salvar): o
 * idioma fica, e os servidores MCP, as skills e os agentes não mudam. */
export function restoreDefaults() {
  const defaults = useSettings.getState().coreSnapshot?.defaults;
  if (defaults) useSettings.setState({ core: defaults });
  updatePrefs(DEFAULT_PREFS);
}

function apply(snapshot: SettingsSnapshot) {
  const { settings, catalog, timeoutRange, contextRange } = snapshot;
  const models = settings.models.map((model) => ({ ...model, uid: uid() }));
  useSettings.setState({
    loaded: true, agents: settings.agents, models, catalog, timeoutRange, contextRange,
    saved: JSON.stringify(payload({ agents: settings.agents, models })),
  });
}

/** Os mods na ordem das abas, acompanhando as configurações. */
export const useAgentIds = () => useSettings(useShallow((state) => agentIds(state.agents)));

export async function loadSettings() {
  try {
    const [agents, core] = await Promise.all([commands.getSettings(), commands.getCoreSettings()]);
    apply(agents);
    applyCore(core);
    checkAllAgents();
  } catch (error) {
    reportError(error);
  }
}

/** Lê de novo a lista do `/model` do agente. O núcleo grava na hora, então
 * só roda sem alteração pendente: aplicar o retrato novo a descartaria. */
export async function refreshModels(agent: AgentId) {
  if (isAgentsDirty(useSettings.getState())) return;
  useSettings.setState({ refreshing: agent });
  try {
    const { snapshot, listed } = await commands.refreshModels(agent);
    apply(snapshot);
    const name = agentLabel(agent);
    if (listed) notify(t("model.refresh.done", { count: snapshot.settings.models.filter((model) => model.agent === agent).length, agent: name }));
    else notify(t("model.refresh.silent", { agent: name }), true);
  } catch (error) {
    reportError(error);
  } finally {
    useSettings.setState({ refreshing: null });
  }
}

export function updateAgent(id: AgentId, patch: Partial<Omit<AgentSettings, "id" | "options">>) {
  useSettings.setState((state) => ({ agents: state.agents.map((agent) => (agent.id === id ? { ...agent, ...patch } : agent)) }));
}

export function updateOptions<A extends AgentId>(id: A, patch: Partial<AgentOptions[A]>) {
  useSettings.setState((state) => ({
    agents: state.agents.map((agent) => (agent.id === id ? { ...agent, options: { ...agent.options, ...patch } } : agent)),
  }));
}

export function updateModel(uid: string, patch: Partial<AgentModel>) {
  useSettings.setState((state) => ({ models: state.models.map((model) => (model.uid === uid ? { ...model, ...patch } : model)) }));
}

export function removeModel(uid: string) {
  const state = useSettings.getState();
  const removed = state.models.find((model) => model.uid === uid);
  useSettings.setState({ models: state.models.filter((model) => model.uid !== uid) });
  // O modelo reserva que saiu da aba sai também das opções.
  if (removed?.agent === "claude") {
    const claude = state.agents.find((agent) => agent.id === "claude") as AgentSettings<"claude"> | undefined;
    if (claude?.options.fallbackModel === removed.model) updateOptions("claude", { fallbackModel: "" });
  }
}

/** O próximo modelo do catálogo que a aba ainda não tem; sem nenhum sobrando,
 * uma linha para digitar. */
export function addModel(agent: AgentId) {
  const { catalog, models } = useSettings.getState();
  const taken = new Set(models.filter((model) => model.agent === agent).map((model) => model.model));
  const known = (catalog[agent] ?? []).find((model) => !taken.has(model.id));
  const owner = useSettings.getState().agents.find((item) => item.id === agent);
  const textOnly = owner ? !editsProject(owner) : isGateway(agent);
  const model: ModelDraft = {
    uid: uid(), agent, enabled: true, capabilities: known?.capabilities ?? (textOnly ? ["chat", "reasoning"] : ["chat", "code", "reasoning", "tools"]),
    model: known?.id ?? "", contextWindow: known?.contextWindow ?? 128000, costClass: known?.costClass ?? "medium", speed: known?.speed ?? "medium",
  };
  useSettings.setState({ models: [...models, model] });
}

/** O que o formulário de Criar mod junta. As linhas vão como a pessoa as
 * escreveu; `model` é o primeiro modelo (os outros se somam na aba do mod). */
export interface ModDraft {
  name: string;
  kind: CustomModOptions["kind"];
  command: string;
  args: string;
  planArgs: string;
  edits: boolean;
  protocol: CustomModOptions["protocol"];
  baseUrl: string;
  apiKey: string;
  keyRequired: boolean;
  model: string;
}

export const EMPTY_MOD: ModDraft = { name: "", kind: "cli", command: "", args: "", planArgs: "", edits: false, protocol: "openai", baseUrl: "", apiKey: "", keyRequired: false, model: "" };

/** Quantos mods criados um ambiente aceita (o `MAX_MODS` do núcleo). */
export const MAX_MODS = 20;

/** Cria o mod no rascunho, ligado e com o primeiro modelo, e devolve o id:
 * como o resto da tela, ele só vale depois do Salvar. */
export function createMod(draft: ModDraft): CustomAgentId {
  const state = useSettings.getState();
  const id = modId(draft.name, state.agents.map((agent) => agent.id));
  const cli = draft.kind === "cli";
  const apiKey = draft.apiKey.trim();
  const options: CustomModOptions = {
    name: draft.name.trim(), kind: draft.kind,
    args: cli ? splitLine(draft.args) : [], planArgs: cli ? splitLine(draft.planArgs) : [], edits: cli && draft.edits,
    protocol: draft.protocol, baseUrl: cli ? "" : draft.baseUrl.trim(), keyRequired: !cli && draft.keyRequired,
    approveMcps: false, hasKey: false, ...(!cli && apiKey ? { apiKey } : {}),
  };
  const agent: AgentSettings = { id, enabled: true, command: cli ? draft.command.trim() : "", timeout: 300, options };
  const name = draft.model.trim();
  const models: ModelDraft[] = name
    ? [{ uid: uid(), agent: id, model: name, enabled: true, capabilities: editsProject(agent) ? ["chat", "code", "reasoning", "tools"] : ["chat", "reasoning"], contextWindow: 128000, costClass: "medium", speed: "medium" }]
    : [];
  useSettings.setState({ agents: [...state.agents, agent], models: [...state.models, ...models] });
  return id;
}

/** Tira o mod criado do rascunho, com os modelos dele. Os do app não saem. */
export function removeMod(id: AgentId) {
  if (!isCustomMod(id)) return;
  useSettings.setState((state) => ({ agents: state.agents.filter((agent) => agent.id !== id), models: state.models.filter((model) => model.agent !== id) }));
}

/** Confere se o executável do agente responde. `quiet` é a conferência de
 * fundo: a linha não pisca em "procurando…" e só muda quando o resultado
 * muda. */
export async function checkAgent(id: AgentId, quiet = false) {
  const agent = useSettings.getState().agents.find((item) => item.id === id);
  if (!agent || !agent.command.trim()) return;
  const setProbe = (probe: ProbeState) => useSettings.setState((state) => (sameProbe(state.probes[id] ?? null, probe) ? state : { probes: { ...state.probes, [id]: probe } }));
  if (!quiet) setProbe("checking");
  let probe: ProbeState;
  try {
    // O login é perguntado só na conferência pedida: na de fundo, a última
    // resposta continua valendo.
    probe = await commands.checkAgent(agent.command, quiet ? null : id);
    const before = useSettings.getState().probes[id];
    if (quiet && before && before !== "checking" && before.path === probe.path && before.loggedIn !== undefined) probe = { ...probe, loggedIn: before.loggedIn };
  } catch {
    probe = { path: null, version: null };
  }
  // O comando mudou enquanto a conferência rodava: o resultado é do antigo.
  if (useSettings.getState().agents.find((item) => item.id === id)?.command !== agent.command) return;
  setProbe(probe);
}

/** Pergunta ao gateway, com o que está gravado, a lista de modelos dele. */
export async function checkGateway(id: AgentId) {
  const set = (value: GatewayCheck | "checking") => useSettings.setState((state) => ({ gateways: { ...state.gateways, [id]: value } }));
  set("checking");
  try {
    set(await commands.checkGateway(id));
  } catch (error) {
    set({ models: 0, error: String(error) });
  }
}

function sameProbe(a: ProbeState, b: ProbeState) {
  if (a === null || b === null || a === "checking" || b === "checking") return a === b;
  return a.path === b.path && a.version === b.version && a.loggedIn === b.loggedIn;
}

/** Confere todos os agentes agora (o botão da página Sistema). */
export function checkAllAgents() {
  for (const agent of cliAgentIds(useSettings.getState().agents)) void checkAgent(agent);
}

/** De quanto em quanto tempo a tela de configurações confere os agentes
 * sozinha: instalar ou remover um CLI aparece sem clicar em nada. */
export const PROBE_EVERY_MS = 10_000;

/** Confere os três agentes agora e depois a cada `PROBE_EVERY_MS`, e também ao
 * voltar para a janela. Devolve quem para tudo. */
export function watchAgents() {
  const all = () => { for (const agent of cliAgentIds(useSettings.getState().agents)) void checkAgent(agent, true); };
  const timer = window.setInterval(() => { if (document.visibilityState === "visible") all(); }, PROBE_EVERY_MS);
  window.addEventListener("focus", all);
  return () => {
    window.clearInterval(timer);
    window.removeEventListener("focus", all);
  };
}

export const MODEL_PATTERN = /^[A-Za-z0-9._:/@[\]-]+$/;

/** Os mesmos erros que o núcleo recusaria, mostrados antes de salvar. As
 * chaves são por campo: `command`, `timeout`, `models`, `budget` e o `uid` de
 * cada modelo. */
export function problems(state: Pick<SettingsState, "agents" | "models">, id: AgentId): Record<string, Key> {
  const found: Record<string, Key> = {};
  const agent = state.agents.find((item) => item.id === id);
  if (!agent) return found;
  const command = agent.command.trim();
  if (isCustomMod(id)) {
    const options = agent.options as CustomModOptions;
    if (!options.name.trim()) found.name = "mods.name.empty";
    if (options.kind === "cli") {
      if (!command) found.command = "agent.command.empty";
      else if (/\s/.test(command)) found.command = "agent.command.hint";
      if (lineProblem(options.args)) found.args = lineProblem(options.args)!;
      if (lineProblem(options.planArgs)) found.planArgs = lineProblem(options.planArgs)!;
      else if (options.edits && options.planArgs.length === 0) found.planArgs = "mods.planArgs.required";
    } else {
      const url = options.baseUrl.trim();
      if (!url) found.baseUrl = "mods.baseUrl.required";
      else if (!/^https?:\/\/[^\s/@]+/i.test(url)) found.baseUrl = "gateway.baseUrl.invalid";
      if (options.keyRequired && agent.enabled && !options.hasKey && !options.apiKey?.trim()) found.apiKey = "gateway.apiKey.required";
      if (options.apiKey && /\s/.test(options.apiKey)) found.apiKey = "gateway.apiKey.invalid";
    }
  } else if (isGateway(id)) {
    const options = (agent as AgentSettings<"openrouter">).options;
    const url = options.baseUrl.trim();
    if (url && !/^https?:\/\/[^\s/@]+/i.test(url)) found.baseUrl = "gateway.baseUrl.invalid";
    if (id === "openrouter" && agent.enabled && !options.hasKey && !options.apiKey?.trim()) found.apiKey = "gateway.apiKey.required";
    if (options.apiKey && /\s/.test(options.apiKey)) found.apiKey = "gateway.apiKey.invalid";
  } else if (!command) found.command = "agent.command.empty";
  else if (/\s/.test(command)) found.command = "agent.command.hint";
  const own = state.models.filter((model) => model.agent === id);
  const seen = new Set<string>();
  for (const model of own) {
    const name = model.model.trim();
    if (!name || !MODEL_PATTERN.test(name)) found[model.uid] = "model.invalid";
    else if (seen.has(name)) found[model.uid] = "model.duplicate";
    else if (model.capabilities.length === 0) found[model.uid] = "model.capabilities.empty";
    seen.add(name);
  }
  if (id === "claude") {
    const budget = (agent as AgentSettings<"claude">).options.maxBudgetUsd;
    if (budget !== null && !(budget > 0 && budget <= 1000)) found.budget = "claude.budget.invalid";
  }
  return found;
}

/** Grava tudo que está pendente em qualquer aba: nada da tela de
 * configurações vale antes deste clique. Cada parte só vai ao núcleo se
 * mudou (salvar o Jev não regrava agentes); uma parte que falha fica
 * pendente, com o motivo na tela. */
export async function saveSettings() {
  const state = useSettings.getState();
  useSettings.setState({ saving: true });
  try {
    if (isAgentsDirty(state)) apply(await commands.saveSettings(payload(state)));
    if (isCoreDirty(state) && state.core) applyCore(await commands.saveCoreSettings(state.core));
    if (!(await saveMcpChanges()) || !(await saveSkillChanges())) return;
    const prefs = changedPrefs(useSettings.getState().prefs, livePrefs());
    useSettings.setState({ prefs: {} });
    // O aviso sai antes de trocar o idioma: depois, as mensagens ainda estão chegando.
    notify(t("settings.saved"));
    if (prefs.theme !== undefined) setThemePreference(prefs.theme);
    if (prefs.tutorialAuto !== undefined) setAutoTours(prefs.tutorialAuto);
    if (prefs.installOnLaunch !== undefined) setInstallOnLaunch(prefs.installOnLaunch);
    if (prefs.locale !== undefined) setLocale(prefs.locale);
    bus.emit("settings:saved", {});
  } catch (error) {
    reportError(error);
  } finally {
    useSettings.setState({ saving: false });
  }
}

/** Só o retrato do Jev, para a página de perfil mostrar o nível. Um
 * rascunho em edição nas configurações continua como está. */
export async function loadCoreSnapshot() {
  try {
    const coreSnapshot = await commands.getCoreSettings();
    if (useSettings.getState().core) useSettings.setState({ coreSnapshot });
    else applyCore(coreSnapshot);
  } catch (error) {
    reportError(error);
  }
}

/** O nível vale na hora, como o idioma: não espera o "Salvar". Só a parte
 * lida do snapshot é trocada — o que estiver sendo editado no Jev continua. */
export async function saveExpertise(level: Expertise) {
  try {
    const coreSnapshot = await commands.saveExpertise(level);
    useSettings.setState({ coreSnapshot });
    notify(t("expertise.saved", { level: t(`expertise.${level}` as Key) }));
  } catch (error) {
    reportError(error);
  }
}

/** A regra de código enxuto vale na hora, como o nível. */
export async function saveLeanCode(enabled: boolean) {
  try {
    const coreSnapshot = await commands.saveLeanCode(enabled);
    useSettings.setState({ coreSnapshot });
    notify(t(enabled ? "expertise.lean.on" : "expertise.lean.off"));
  } catch (error) {
    reportError(error);
  }
}

/** Joga fora tudo que está pendente, em todas as abas, e relê o gravado. */
export function discardChanges() {
  useSettings.setState({ prefs: {} });
  void loadSettings();
  discardMcp();
  discardSkills();
}

/** Ao abrir a tela: relê cada parte que não tem mudança pendente. Voltar à
 * página com alteração pendente não a joga fora. */
function refreshSettings() {
  const state = useSettings.getState();
  if (!state.loaded || !isDirty(state)) void loadSettings();
  if (!isMcpDirty(useMcp.getState())) void loadMcp();
  if (!isSkillsDirty(useSkills.getState())) void loadSkills();
}

export function connectSettings() {
  let stop: (() => void) | null = null;
  const off = bus.on("view:changed", ({ view }) => {
    stop?.();
    stop = null;
    if (view === "profile") void loadCoreSnapshot();
    // A página Sistema mostra se cada agente responde: confere ao abrir, sem
    // reler a configuração quando ela já está carregada.
    if (view === "status") {
      if (useSettings.getState().loaded) checkAllAgents();
      else void loadSettings();
    }
    if (view !== "settings") return;
    refreshSettings();
    stop = watchAgents();
  });
  // A descoberta de fundo trocou os modelos: a aba aberta e sem alteração
  // pendente mostra a lista nova.
  const models = onCore("models-updated", () => {
    const state = useSettings.getState();
    if (state.loaded && !isAgentsDirty(state)) void commands.getSettings().then(apply).catch(reportError);
  });
  // Salvar vale para o app inteiro: o idioma é dito de novo ao núcleo, e quem
  // olha o que foi salvo (a página Sistema) o relê. Com um pedido no ar, a
  // troca fica para o próximo — e a pessoa é avisada disso.
  const applied = onCore("settings-applied", ({ now }) => {
    syncLanguage();
    if (!now) notify(t("settings.saved.later"), true);
  });
  return () => { stop?.(); off(); void models.then((unlisten) => unlisten()); void applied.then((unlisten) => unlisten()); };
}
