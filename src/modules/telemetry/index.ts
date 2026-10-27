import { create } from "zustand";
import { useAuth } from "@/modules/auth";
import { supabase } from "@/modules/auth/client";
import { bus } from "@/modules/core";
import { useChatFind } from "@/modules/conversation";
import { useLive } from "@/modules/live";
import { VIEW_FEATURE, settingsTabFeature } from "@/modules/plans";
import { useSettingsTab } from "@/modules/settings";
import { useNavigation } from "@/modules/navigation";
import { Counter, MEASURED, type MeasuredFeature } from "./counter";

/** A telemetria opcional dos recursos: só com o consentimento de quem usa, o
 * app conta quantas vezes cada recurso foi aberto por dia e manda as contagens
 * (sem texto, caminho nem nome de projeto) a cada 5 minutos. O consentimento
 * mora no servidor (`usage_telemetry`), e desligar apaga o que foi enviado. */
interface TelemetryState {
  /** `null` enquanto não leu do servidor. */
  granted: boolean | null;
  saving: boolean;
}

export const useTelemetry = create<TelemetryState>(() => ({ granted: null, saving: false }));

const counter = new Counter();
const FLUSH_EVERY = 5 * 60 * 1000;

const isMeasured = (feature: string | undefined): feature is MeasuredFeature => !!feature && (MEASURED as readonly string[]).includes(feature);

/** Conta um uso do recurso, se a pessoa aceitou. */
export function track(feature: string | undefined) {
  if (useTelemetry.getState().granted !== true || !isMeasured(feature)) return;
  counter.add(feature);
}

export async function loadConsent() {
  const { data, error } = await supabase.from("usage_telemetry").select("user_id").maybeSingle();
  useTelemetry.setState({ granted: error ? false : data !== null });
}

/** Liga ou desliga o envio. Desligar apaga no servidor o que já foi enviado. */
export async function setConsent(granted: boolean) {
  useTelemetry.setState({ saving: true });
  try {
    const { error } = await supabase.rpc("set_usage_telemetry", { granted });
    if (error) throw new Error(error.message);
    if (!granted) counter.drain();
    useTelemetry.setState({ granted });
  } finally {
    useTelemetry.setState({ saving: false });
  }
}

/** Manda o que foi contado. Uma falha devolve as contagens ao contador. */
export async function flush() {
  if (useTelemetry.getState().granted !== true || counter.size === 0) return;
  const items = counter.drain();
  const { error } = await supabase.rpc("record_feature_usage", { items });
  if (error) counter.restore(items);
}

export function connectTelemetry() {
  const reload = () => void loadConsent().catch(() => useTelemetry.setState({ granted: false }));
  if (useAuth.getState().status === "signedIn") reload();
  // A conta que entra (ou troca) lê o consentimento dela; quem sai não manda nada.
  const offAuth = useAuth.subscribe((state, previous) => {
    if (state.status === previous.status && state.email === previous.email) return;
    if (state.status === "signedIn") reload();
    else useTelemetry.setState({ granted: null });
  });
  const timer = setInterval(() => void flush(), FLUSH_EVERY);
  const offView = bus.on("view:changed", ({ view }) => track(VIEW_FEATURE[view]));
  const offTab = useSettingsTab.subscribe((state, previous) => {
    if (state.tab !== previous.tab && useNavigation.getState().view === "settings") track(settingsTabFeature(state.tab));
  });
  const offLive = useLive.subscribe((state, previous) => {
    if (Object.entries(state.panel).some(([chat, open]) => open && !previous.panel[chat])) track("liveFiles");
  });
  const offFind = useChatFind.subscribe((state, previous) => {
    if (state.open && !previous.open) track("conversationFind");
  });
  const leave = () => void flush();
  window.addEventListener("beforeunload", leave);
  return () => {
    clearInterval(timer);
    offAuth();
    offView();
    offTab();
    offLive();
    offFind();
    window.removeEventListener("beforeunload", leave);
  };
}

export { Counter, MEASURED, type MeasuredFeature, type UsageItem } from "./counter";
