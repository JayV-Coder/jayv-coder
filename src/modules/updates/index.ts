import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";
import { onCore } from "@/modules/core";
import { resumedFromSwitch } from "@/modules/environments/switch";

/** Em que pé está a atualização. `available` é a versão nova achada sozinha,
 * esperando a pessoa mandar instalar (o aviso no topo e a notificação).
 * `available`, `latest` e `failed` são os únicos pontos em que a janela pode
 * ser fechada: no meio do download ou da instalação o aplicativo está
 * trocando os próprios arquivos. */
export type UpdatePhase = "idle" | "checking" | "available" | "latest" | "downloading" | "installing" | "restarting" | "failed";
export type UpdateStep = "checking" | "downloading" | "installing" | "restarting";

interface UpdateState {
  phase: UpdatePhase;
  open: boolean;
  /** A versão instalada e a que está chegando. */
  current: string | null;
  next: string | null;
  /** As notas do release, em Markdown, quando o servidor as manda. */
  notes: string | null;
  date: string | null;
  /** Bytes baixados e o total, quando o servidor diz o tamanho. */
  received: number;
  total: number | null;
  error: string | null;
  /** O passo em que a falha aconteceu. */
  failedAt: UpdateStep | null;
  /** A versão cujo aviso no topo a pessoa fechou: a mesma não volta a
   * aparecer até reabrir o app; uma mais nova, sim. */
  dismissed: string | null;
  /** Quando a última consulta voltou com resposta (ISO), com ou sem versão nova. */
  checkedAt: string | null;
  /** A consulta de quando o app abre está em curso: a tela de abertura espera por ela. */
  launching: boolean;
  /** Instalar sozinho a versão nova achada ao abrir o app (ligado por padrão). */
  installOnLaunch: boolean;
}

const LAUNCH_KEY = "jayv.updates.launch";

function savedInstallOnLaunch(): boolean {
  try { return localStorage.getItem(LAUNCH_KEY) !== "off"; } catch { return true; }
}

/** A consulta ao voltar para a janela não se repete em menos que isto: quem
 * pergunta de tempos em tempos é o núcleo (`tray::watch_updates`), que segue
 * perguntando com a janela escondida na bandeja. */
export const RECHECK_MS = 15_000;

export const useUpdate = create<UpdateState>(() => ({
  phase: "idle", open: false, current: null, next: null, notes: null, date: null, received: 0, total: null, error: null, failedAt: null, dismissed: null,
  checkedAt: null, launching: false, installOnLaunch: savedInstallOnLaunch(),
}));

/** Quanto a consulta de abertura espera antes de seguir sem ela: sem rede o
 * app não fica parado numa tela de espera. */
export const LAUNCH_TIMEOUT_MS = 10_000;

export function setInstallOnLaunch(on: boolean) {
  try { localStorage.setItem(LAUNCH_KEY, on ? "on" : "off"); } catch { /* sem armazenamento: vale só nesta sessão */ }
  useUpdate.setState({ installOnLaunch: on });
}

let launched = false;

/** Ao abrir o app: consulta antes de mostrar a tela e, havendo versão nova,
 * baixa, instala e reinicia sozinho, com a janela de progresso na frente.
 * Sem rede, passado o tempo, numa build de desenvolvimento ou com a opção
 * desligada, o app abre normalmente (a versão achada vira o aviso do topo).
 * Roda uma vez por abertura. */
export async function updateAtLaunch() {
  if (launched) return;
  launched = true;
  // Recarregada por uma troca de ambiente: o app já está aberto e atualizado.
  if (resumedFromSwitch()) return;
  useUpdate.setState({ launching: true });
  let skipped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<null>((resolve) => { timer = setTimeout(() => { skipped = true; resolve(null); }, LAUNCH_TIMEOUT_MS); });
  const asking = check().catch((error) => { console.warn("launch update check failed", error); return null; });
  const update = await Promise.race([asking, timeout]);
  clearTimeout(timer);
  useUpdate.setState({ launching: false });
  if (!update) {
    // O que a consulta lenta achar depois do limite ainda vira aviso.
    if (skipped) void asking.then((late) => { if (late) keep(late); });
    else useUpdate.setState({ checkedAt: answered() });
    return;
  }
  useUpdate.setState({ checkedAt: answered() });
  if (pending) void pending.close().catch(() => undefined);
  pending = update;
  if (useUpdate.getState().installOnLaunch) await install(update);
  else keep(update);
}

/** Guarda a versão achada como aviso do topo, sem abrir janela. */
function keep(update: Update) {
  if (useUpdate.getState().phase !== "idle" && useUpdate.getState().phase !== "latest") { void update.close().catch(() => undefined); return; }
  if (pending && pending !== update) void pending.close().catch(() => undefined);
  pending = update;
  useUpdate.setState({
    phase: "available", current: update.currentVersion, next: update.version,
    notes: update.body?.trim() || null, date: update.date ?? null, error: null, failedAt: null,
  });
}

const answered = () => new Date().toISOString();

/** A versão achada pela consulta silenciosa, guardada até a pessoa mandar
 * instalar. */
let pending: Update | null = null;

const busy = (phase: UpdatePhase) => phase === "checking" || phase === "downloading" || phase === "installing" || phase === "restarting";


/** Pergunta ao repositório de releases se há versão nova. Ao abrir o
 * aplicativo, quando o núcleo acha versão nova e ao voltar para a janela
 * (`announce` falso) nada se instala sozinho:
 * a versão nova vira `available` (o aviso no topo e a notificação), e sem
 * rede ou numa build de desenvolvimento a falha fica calada. Pelo botão
 * (`announce`) a janela aparece desde a consulta, diz também "já está na mais
 * nova" e o erro, e a versão achada espera na janela a pessoa
 * escolher entre atualizar agora ou depois: nada se instala sem ela mandar. */
export async function checkForUpdate(announce = false) {
  const state = useUpdate.getState();
  // Uma atualização em curso não começa outra: o botão só reabre a janela.
  if (busy(state.phase)) { if (announce) useUpdate.setState({ open: true }); return; }
  if (!announce) { await checkQuietly(); return; }
  if (pending) { offer(pending); return; }
  useUpdate.setState({ phase: "checking", open: true, error: null, failedAt: null, received: 0, total: null, next: null, notes: null, date: null });
  let update: Update | null;
  try {
    update = await check();
  } catch (error) {
    console.warn("update check failed", error);
    useUpdate.setState({ phase: "failed", error: String(error), failedAt: "checking" });
    return;
  }
  if (!update) { useUpdate.setState({ phase: "latest", checkedAt: answered() }); return; }
  // A consulta silenciosa pode ter guardado outra enquanto esta esperava.
  const previous = pending as Update | null;
  if (previous && previous !== update) void previous.close().catch(() => undefined);
  pending = update;
  useUpdate.setState({ checkedAt: answered() });
  offer(update);
}

/** A versão achada na janela aberta, com o que muda, esperando a escolha. */
function offer(update: Update) {
  useUpdate.setState({
    phase: "available", open: true, current: update.currentVersion, next: update.version,
    notes: update.body?.trim() || null, date: update.date ?? null, error: null, failedAt: null, received: 0, total: null,
  });
}

/** A consulta sem janela: não mexe na fase enquanto pergunta (o aviso do topo
 * não pisca), e o que volta depois que a pessoa já mandou atualizar é
 * descartado. */
async function checkQuietly() {
  let update: Update | null;
  try {
    update = await check();
  } catch (error) {
    console.warn("update check failed", error);
    return;
  }
  useUpdate.setState({ checkedAt: answered() });
  const { phase } = useUpdate.getState();
  if (phase !== "idle" && phase !== "available" && phase !== "latest") { void update?.close().catch(() => undefined); return; }
  if (pending) void pending.close().catch(() => undefined);
  pending = update;
  if (!update) {
    if (phase === "available") useUpdate.setState({ phase: "idle" });
    return;
  }
  useUpdate.setState({
    phase: "available", current: update.currentVersion, next: update.version,
    notes: update.body?.trim() || null, date: update.date ?? null, error: null, failedAt: null,
  });
}

/** Instala a versão já achada (o botão do aviso e da janela); sem uma
 * guardada, consulta e mostra o que achou, para a pessoa escolher. */
export async function installUpdate() {
  if (pending && !busy(useUpdate.getState().phase)) await install(pending);
  else await checkForUpdate(true);
}

/** Abre a janela com a versão nova e o que muda, sem instalar ainda. */
export function showUpdate() {
  if (useUpdate.getState().phase === "available" || busy(useUpdate.getState().phase)) useUpdate.setState({ open: true });
  else void checkForUpdate(true);
}

/** Esconde o aviso do topo para esta versão; a notificação fica no sino. */
export function dismissUpdate() {
  useUpdate.setState((state) => ({ dismissed: state.next }));
}

/** Ao abrir, `updateAtLaunch`; depois, quando o núcleo avisa que achou versão nova, ao voltar
 * a rede (o computador acordou, o Wi-Fi voltou) e ao voltar para a janela.
 * Quem pergunta de tempos em tempos é o núcleo: os relógios da tela param
 * quando a janela vai para a bandeja. Uma consulta que ainda não voltou não
 * ganha outra por cima. */
export function connectUpdates() {
  let asking = false;
  let last = 0;
  const ask = () => {
    if (asking) return;
    asking = true;
    last = Date.now();
    void checkForUpdate().finally(() => { asking = false; });
  };
  const back = () => { if (document.visibilityState === "visible" && Date.now() - last >= RECHECK_MS) ask(); };
  last = Date.now();
  void updateAtLaunch();
  const off = onCore("update-found", ask);
  window.addEventListener("online", ask);
  window.addEventListener("focus", back);
  document.addEventListener("visibilitychange", back);
  return () => {
    void off.then((unlisten) => unlisten());
    window.removeEventListener("online", ask);
    window.removeEventListener("focus", back);
    document.removeEventListener("visibilitychange", back);
  };
}

async function install(update: Update) {
  useUpdate.setState({
    phase: "downloading", open: true, current: update.currentVersion, next: update.version,
    notes: update.body?.trim() || null, date: update.date ?? null, received: 0, total: null, error: null,
  });
  try {
    await update.download((event) => {
      if (event.event === "Started") useUpdate.setState({ total: event.data.contentLength ?? null, received: 0 });
      else if (event.event === "Progress") useUpdate.setState((state) => ({ received: state.received + event.data.chunkLength }));
    });
    useUpdate.setState({ phase: "installing" });
    await update.install();
    useUpdate.setState({ phase: "restarting" });
    await relaunch();
  } catch (error) {
    console.error("update failed", error);
    const at = useUpdate.getState().phase;
    useUpdate.setState({ phase: "failed", error: String(error), failedAt: at === "installing" || at === "restarting" ? at : "downloading" });
  } finally {
    if (pending === update) pending = null;
    void update.close().catch(() => undefined);
  }
}

/** Fecha a janela, quando ela pode ser fechada. A versão nova ainda não
 * instalada continua esperando no aviso do topo. */
export function closeUpdate() {
  const { phase } = useUpdate.getState();
  if (busy(phase)) return;
  useUpdate.setState({ open: false, phase: phase === "available" ? "available" : "idle" });
}

export function isUpdateBusy(phase: UpdatePhase) { return busy(phase); }
