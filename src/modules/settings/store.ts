import { create } from "zustand";
import {
  bus, commands, onCore, type AgentId, type AgentModel, type AgentOptions, type AgentProbe, type AgentSettings, type GatewayCheck, type KnownModel, isGateway,
  type CoreSettings, type CoreSnapshot, type Expertise, type SettingsSnapshot,
} from "@/modules/core";
import { notify, reportError } from "@/modules/feedback";
import { syncLanguage, t, type Key } from "@/modules/i18n";

export const AGENTS: AgentId[] = ["claude", "codex", "copilot", "cursor", "kilo", "openrouter", "litellm"];
export const AGENT_LABELS: Record<AgentId, string> = {
  claude: "Claude Code", codex: "Codex", copilot: "GitHub Copilot", cursor: "Cursor", kilo: "Kilo Code", openrouter: "OpenRouter", litellm: "LiteLLM",
};
/** Os agentes que são um programa na máquina: só neles há o que conferir
 * (instalado, versão, login). Os gateways de API têm endereço e chave. */
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
}

let next = 0;
const uid = () => `model-${++next}`;
const noProbes = (): Record<AgentId, ProbeState> => ({ claude: null, codex: null, copilot: null, cursor: null, kilo: null, openrouter: null, litellm: null });

export const useSettings = create<SettingsState>(() => ({
  loaded: false, agents: [], models: [], catalog: { claude: [], codex: [], copilot: [], cursor: [], kilo: [], openrouter: [], litellm: [] }, timeoutRange: [30, 3600],
  contextRange: [8000, 2000000], probes: noProbes(), gateways: {}, saved: "", saving: false, refreshing: null,
  core: null, coreSnapshot: null, savedCore: "",
}));

const payload = ({ agents, models }: Pick<SettingsState, "agents" | "models">) =>
  ({ agents, models: models.map(({ uid: _, ...model }) => model) });

export const isAgentsDirty = (state: SettingsState) => state.loaded && JSON.stringify(payload(state)) !== state.saved;
export const isCoreDirty = (state: SettingsState) => state.core !== null && JSON.stringify(state.core) !== state.savedCore;
export const isDirty = (state: SettingsState) => isAgentsDirty(state) || isCoreDirty(state);

function applyCore(snapshot: CoreSnapshot) {
  useSettings.setState({ core: snapshot.settings, coreSnapshot: snapshot, savedCore: JSON.stringify(snapshot.settings) });
}

/** Muda um campo das abas do Jev e do app. */
export function updateCore(patch: Partial<CoreSettings>) {
  useSettings.setState((state) => (state.core ? { core: { ...state.core, ...patch } } : state));
}

/** Volta as abas do Jev e do app aos valores de partida (sem salvar). */
export function restoreCoreDefaults() {
  const defaults = useSettings.getState().coreSnapshot?.defaults;
  if (defaults) useSettings.setState({ core: defaults });
}

function apply(snapshot: SettingsSnapshot) {
  const { settings, catalog, timeoutRange, contextRange } = snapshot;
  const models = settings.models.map((model) => ({ ...model, uid: uid() }));
  useSettings.setState({
    loaded: true, agents: settings.agents, models, catalog, timeoutRange, contextRange,
    saved: JSON.stringify(payload({ agents: settings.agents, models })),
  });
}

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
    const name = AGENT_LABELS[agent];
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
  const known = catalog[agent].find((model) => !taken.has(model.id));
  const model: ModelDraft = {
    uid: uid(), agent, enabled: true, capabilities: known?.capabilities ?? (isGateway(agent) ? ["chat", "reasoning"] : ["chat", "code", "reasoning", "tools"]),
    model: known?.id ?? "", contextWindow: known?.contextWindow ?? 128000, costClass: known?.costClass ?? "medium", speed: known?.speed ?? "medium",
  };
  useSettings.setState({ models: [...models, model] });
}

/** Confere se o executável do agente responde. `quiet` é a conferência de
 * fundo: a linha não pisca em "procurando…" e só muda quando o resultado
 * muda. */
export async function checkAgent(id: AgentId, quiet = false) {
  const agent = useSettings.getState().agents.find((item) => item.id === id);
  if (!agent || !agent.command.trim()) return;
  const setProbe = (probe: ProbeState) => useSettings.setState((state) => (sameProbe(state.probes[id], probe) ? state : { probes: { ...state.probes, [id]: probe } }));
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
  for (const agent of CLI_AGENTS) void checkAgent(agent);
}

/** De quanto em quanto tempo a tela de configurações confere os agentes
 * sozinha: instalar ou remover um CLI aparece sem clicar em nada. */
export const PROBE_EVERY_MS = 10_000;

/** Confere os três agentes agora e depois a cada `PROBE_EVERY_MS`, e também ao
 * voltar para a janela. Devolve quem para tudo. */
export function watchAgents() {
  const all = () => { for (const agent of CLI_AGENTS) void checkAgent(agent, true); };
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
  if (isGateway(id)) {
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

export async function saveSettings() {
  const state = useSettings.getState();
  useSettings.setState({ saving: true });
  try {
    // Cada parte só vai ao núcleo se mudou: salvar o Jev não regrava agentes.
    if (isAgentsDirty(state)) apply(await commands.saveSettings(payload(state)));
    if (isCoreDirty(state) && state.core) applyCore(await commands.saveCoreSettings(state.core));
    notify(t("settings.saved"));
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

export function discardChanges() {
  void loadSettings();
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
    // Voltar à página com alteração pendente não a joga fora.
    if (!isDirty(useSettings.getState())) void loadSettings();
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
