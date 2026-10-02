import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";

/** Em que pé está a atualização. `latest` e `failed` são os únicos pontos em
 * que a janela pode ser fechada: no meio do download ou da instalação o
 * aplicativo está trocando os próprios arquivos. */
export type UpdatePhase = "idle" | "checking" | "latest" | "downloading" | "installing" | "restarting" | "failed";
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
}

export const useUpdate = create<UpdateState>(() => ({
  phase: "idle", open: false, current: null, next: null, notes: null, date: null, received: 0, total: null, error: null, failedAt: null,
}));

const busy = (phase: UpdatePhase) => phase === "checking" || phase === "downloading" || phase === "installing" || phase === "restarting";

/** Pergunta ao repositório de releases se há versão nova e, havendo, já
 * atualiza, mostrando cada passo numa janela. Ao abrir o aplicativo
 * (`announce` falso) a janela só aparece se houver o que instalar: sem rede ou
 * numa build de desenvolvimento a falha fica calada. Pelo botão a janela
 * aparece desde a consulta e diz também "já está na mais nova" e o erro. */
export async function checkForUpdate(announce = false) {
  const state = useUpdate.getState();
  // Uma atualização em curso não começa outra: o botão só reabre a janela.
  if (busy(state.phase)) { useUpdate.setState({ open: true }); return; }
  useUpdate.setState({ phase: "checking", open: announce, error: null, failedAt: null, received: 0, total: null, next: null, notes: null, date: null });
  let update: Update | null;
  try {
    update = await check();
  } catch (error) {
    console.warn("update check failed", error);
    useUpdate.setState({ phase: announce ? "failed" : "idle", error: String(error), failedAt: "checking" });
    return;
  }
  if (!update) {
    useUpdate.setState({ phase: announce ? "latest" : "idle" });
    return;
  }
  await install(update);
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
    void update.close().catch(() => undefined);
  }
}

/** Fecha a janela, quando ela pode ser fechada. */
export function closeUpdate() {
  if (busy(useUpdate.getState().phase)) return;
  useUpdate.setState({ open: false, phase: "idle" });
}

export function isUpdateBusy(phase: UpdatePhase) { return busy(phase); }
