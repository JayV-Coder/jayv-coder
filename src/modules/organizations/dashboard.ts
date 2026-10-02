import { create } from "zustand";
import { commands, onCore, type GateFeed, type UsageReport } from "@/modules/core";
import { periodBounds, type Period, type Range } from "@/modules/usage";
import { dashboardGateScope, dashboardScope, type DashboardFilter } from "./filter";

const EMPTY_FEED: GateFeed = { entries: [], exits: [], tally: { passed: 0, asked: 0, blocked: 0, held: 0 } };
const today = () => new Date().toLocaleDateString("en-CA");

interface DashboardState {
  /** A organização do recorte: abrir outra zera o filtro. */
  orgId: string | null;
  filter: DashboardFilter;
  period: Period;
  range: Range;
  report: UsageReport | null;
  feed: GateFeed;
  /** Os projetos da organização na última leitura, para a releitura ao vivo. */
  projectIds: string[];
}

export const useDashboard = create<DashboardState>(() => ({
  orgId: null,
  filter: { projectId: null, chatId: null },
  period: "30d",
  range: { from: today(), to: today() },
  report: null,
  feed: EMPTY_FEED,
  projectIds: [],
}));

/** Prepara o recorte para uma organização. A mesma organização mantém o
 * filtro escolhido; outra começa por todos os projetos. */
export function openDashboard(orgId: string, projectIds: string[]) {
  const state = useDashboard.getState();
  if (state.orgId === orgId) { useDashboard.setState({ projectIds }); return; }
  useDashboard.setState({ orgId, projectIds, filter: { projectId: null, chatId: null }, report: null, feed: EMPTY_FEED });
}

export function setDashboardFilter(filter: DashboardFilter) {
  useDashboard.setState({ filter, report: null, feed: EMPTY_FEED });
}

export function setDashboardPeriod(period: Period, range?: Range) {
  const fresh = period === "custom" && !range && useDashboard.getState().period !== "custom" ? { from: today(), to: today() } : undefined;
  const next = range ?? fresh;
  useDashboard.setState(next ? { period, range: next } : { period });
}

function same(a: DashboardState, b: DashboardState) {
  return a.orgId === b.orgId && a.filter === b.filter && a.period === b.period && a.range === b.range && a.projectIds === b.projectIds;
}

export async function loadDashboardReport() {
  const asked = useDashboard.getState();
  if (!asked.orgId) return;
  const report = await commands.usageReport({
    scope: dashboardScope(asked.filter, asked.projectIds),
    ...periodBounds(asked.period, asked.range),
    utcOffsetMinutes: -new Date().getTimezoneOffset(),
  });
  // Uma troca de recorte no meio da consulta não recebe a conta velha.
  if (same(useDashboard.getState(), asked)) useDashboard.setState({ report });
}

export async function loadDashboardGate() {
  const asked = useDashboard.getState();
  if (!asked.orgId) return;
  const { projectIds, chatId } = dashboardGateScope(asked.filter, asked.projectIds);
  const feed = await commands.scopedGateFeed(projectIds, chatId);
  if (same(useDashboard.getState(), asked)) useDashboard.setState({ feed });
}

const SETTLE = 800;

/** Enquanto uma das duas abas está aberta, gasto novo e pedido novo na
 * portaria relêem a vista, com a mesma folga das estatísticas. Quem diz se a
 * aba está aberta é quem chama. */
export function connectDashboard(visible: () => "stats" | "gate" | null) {
  let settle: ReturnType<typeof setTimeout> | null = null;
  const later = () => {
    if (!visible()) return;
    if (settle) clearTimeout(settle);
    settle = setTimeout(() => {
      settle = null;
      const tab = visible();
      if (tab === "stats") void loadDashboardReport().catch(console.error);
      if (tab === "gate") void loadDashboardGate().catch(console.error);
    }, SETTLE);
  };
  const offs = [onCore("usage-recorded", later), onCore("gate-entry", later), onCore("gate-exit", later)];
  return () => {
    if (settle) clearTimeout(settle);
    offs.forEach((off) => void off.then((unlisten) => unlisten()));
  };
}
