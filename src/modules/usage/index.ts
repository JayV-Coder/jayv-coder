import { create } from "zustand";
import { bus, commands, onCore, type QuotaStatus, type TurnUsage, type UsageReport, type UsageScope } from "@/modules/core";
import { reportError } from "@/modules/feedback";
import { navigate, useNavigation } from "@/modules/navigation";

export type Period = "today" | "7d" | "30d" | "all" | "custom";
/** O intervalo escolhido à mão, em datas locais `AAAA-MM-DD`, as duas
 * pontas inclusas. */
export interface Range { from: string; to: string }

interface UsageState {
  scope: UsageScope;
  period: Period;
  range: Range;
  report: UsageReport | null;
  loading: boolean;
  /** O motivo de cada agente cujo limite não pôde ser lido. */
  quotaProblems: QuotaStatus[];
  refreshingQuotas: boolean;
  /** O gasto de cada turno, por chat, para o rodapé dos balões. */
  turns: Record<string, Record<string, TurnUsage>>;
  /** A conta inteira de cada chat, para o rodapé da conversa. */
  chats: Record<string, UsageReport>;
  /** Os últimos 30 dias de cada projeto, para o cartão dele. */
  projects: Record<string, UsageReport>;
  /** Os últimos 30 dias da conta inteira, para a página de perfil. */
  account: UsageReport | null;
}

/** A data de hoje no fuso de quem lê: `toISOString` daria o dia em UTC, e
 * depois das 21h no Brasil o intervalo começaria amanhã. */
const today = () => new Date().toLocaleDateString("en-CA");

export const useUsage = create<UsageState>(() => ({
  scope: { kind: "global" },
  period: "30d",
  range: { from: today(), to: today() },
  report: null,
  loading: false,
  quotaProblems: [],
  refreshingQuotas: false,
  turns: {},
  chats: {},
  projects: {},
  account: null,
}));

function midnight(daysAgo: number) {
  const date = new Date();
  date.setHours(0, 0, 0, 0);
  date.setDate(date.getDate() - daysAgo);
  return date;
}

function localDay(day: string, plusDays = 0) {
  const [year, month, date] = day.split("-").map(Number);
  return new Date(year, month - 1, date + plusDays);
}

/** O intervalo do período em ISO. "7 dias" são hoje e os seis dias antes. */
export function periodBounds(period: Period, range: Range): { from: string | null; to: string | null } {
  switch (period) {
    case "today": return { from: midnight(0).toISOString(), to: null };
    case "7d": return { from: midnight(6).toISOString(), to: null };
    case "30d": return { from: midnight(29).toISOString(), to: null };
    case "all": return { from: null, to: null };
    case "custom": return { from: localDay(range.from).toISOString(), to: localDay(range.to, 1).toISOString() };
  }
}

const offset = () => -new Date().getTimezoneOffset();

function query(scope: UsageScope, period: Period, range: Range) {
  return { scope, ...periodBounds(period, range), utcOffsetMinutes: offset() };
}

export async function loadReport() {
  const { scope, period, range } = useUsage.getState();
  useUsage.setState({ loading: true });
  try {
    const report = await commands.usageReport(query(scope, period, range));
    // Uma troca de escopo no meio da consulta não pode receber a conta velha.
    const now = useUsage.getState();
    if (now.scope === scope && now.period === period && now.range === range) useUsage.setState({ report });
  } catch (error) {
    reportError(error);
  } finally {
    useUsage.setState({ loading: false });
  }
}

export function setScope(scope: UsageScope) {
  useUsage.setState({ scope });
  void loadReport();
}

export function setPeriod(period: Period, range?: Range) {
  // O intervalo personalizado nasce em hoje, contado na hora da escolha.
  const fresh = period === "custom" && !range && useUsage.getState().period !== "custom" ? { from: today(), to: today() } : undefined;
  const next = range ?? fresh;
  useUsage.setState(next ? { period, range: next } : { period });
  void loadReport();
}

/** Abre as estatísticas já recortadas: o clique no rodapé do chat ou numa
 * linha da tabela. */
export function openStats(scope: UsageScope) {
  useUsage.setState({ scope });
  navigate("stats");
}

export async function refreshQuotas() {
  useUsage.setState({ refreshingQuotas: true });
  try {
    useUsage.setState({ quotaProblems: await commands.refreshQuotas() });
  } catch (error) {
    reportError(error);
  } finally {
    useUsage.setState({ refreshingQuotas: false });
  }
}

export async function loadChatUsage(chatId: string) {
  try {
    const [turns, report] = await Promise.all([
      commands.chatUsage(chatId),
      commands.usageReport(query({ kind: "chat", id: chatId }, "all", useUsage.getState().range)),
    ]);
    useUsage.setState((state) => ({
      turns: { ...state.turns, [chatId]: Object.fromEntries(turns.map((turn) => [turn.turnId, turn])) },
      chats: { ...state.chats, [chatId]: report },
    }));
  } catch (error) {
    reportError(error);
  }
}

export async function loadProjectUsage(projectId: string) {
  try {
    const report = await commands.usageReport(query({ kind: "project", id: projectId }, "30d", useUsage.getState().range));
    useUsage.setState((state) => ({ projects: { ...state.projects, [projectId]: report } }));
  } catch (error) {
    reportError(error);
  }
}

export async function loadAccountUsage() {
  try {
    const account = await commands.usageReport(query({ kind: "global" }, "30d", useUsage.getState().range));
    useUsage.setState({ account });
  } catch (error) {
    reportError(error);
  }
}

/** Quanto do total não foi informado pela ferramenta, de 0 a 1. */
export function estimatedShare(totals: UsageReport["totals"]) {
  const all = totals.inputTokens + totals.outputTokens;
  return all === 0 ? 0 : totals.estimatedTokens / all;
}

const QUOTA_EVERY = 15 * 60 * 1000;
const SETTLE = 800;

/** As estatísticas se refazem quando entra gasto novo — com folga, porque um
 * turno grava vários registros em sequência — e o limite dos planos é relido
 * a cada 15 minutos enquanto a tela delas está aberta. */
export function connectUsage() {
  let quotaTimer: ReturnType<typeof setInterval> | null = null;
  let settle: ReturnType<typeof setTimeout> | null = null;
  const touched = { chats: new Set<string>(), projects: new Set<string>() };

  const flush = () => {
    settle = null;
    const state = useUsage.getState();
    const view = useNavigation.getState().view;
    if (view === "stats") void loadReport();
    if (view === "profile") void loadAccountUsage();
    for (const chatId of touched.chats) if (state.chats[chatId] || state.turns[chatId]) void loadChatUsage(chatId);
    for (const projectId of touched.projects) if (state.projects[projectId]) void loadProjectUsage(projectId);
    touched.chats.clear();
    touched.projects.clear();
  };
  const later = () => {
    if (settle) clearTimeout(settle);
    settle = setTimeout(flush, SETTLE);
  };

  const offRecorded = onCore("usage-recorded", ({ chatId, projectId }) => {
    if (chatId) touched.chats.add(chatId);
    if (projectId) touched.projects.add(projectId);
    later();
  });
  const offQuota = onCore("quota-changed", () => later());
  const offView = bus.on("view:changed", ({ view }) => {
    if (quotaTimer) { clearInterval(quotaTimer); quotaTimer = null; }
    if (view === "profile") void loadAccountUsage();
    if (view !== "stats") return;
    void loadReport();
    quotaTimer = setInterval(() => void refreshQuotas(), QUOTA_EVERY);
  });

  return () => {
    offView();
    if (quotaTimer) clearInterval(quotaTimer);
    if (settle) clearTimeout(settle);
    void offRecorded.then((unlisten) => unlisten());
    void offQuota.then((unlisten) => unlisten());
  };
}

export { agentLabel, formatCost, formatDuration, formatPercent, formatReset, formatTokens, sourceColor, sourceLabel, windowLabel } from "./format";
