import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import logo from "./assets/logo.svg?raw";
import lamp from "./assets/lamp.svg?raw";
import signalHead3 from "./assets/signal-head-3.svg?raw";
import signalHead2 from "./assets/signal-head-2.svg?raw";
import iconGateIn from "./assets/icon-gate-in.svg?raw";
import iconGateOut from "./assets/icon-gate-out.svg?raw";
import iconCommand from "./assets/icon-command.svg?raw";
import iconFile from "./assets/icon-file.svg?raw";
import iconHouseRule from "./assets/icon-house-rule.svg?raw";
import iconChevron from "./assets/icon-chevron.svg?raw";
import iconChatRef from "./assets/icon-chat-ref.svg?raw";
import iconFolder from "./assets/icon-folder.svg?raw";

const ENTRY_VERDICTS = {
  pass: { aspect: "go", label: "passou" },
  ask: { aspect: "ask", label: "vai perguntar" },
  block: { aspect: "stop", label: "barrado" },
};
const EXIT_VERDICTS = {
  cleared: { aspect: "go", label: "liberado" },
  held: { aspect: "stop", label: "segurado" },
};
const TALLY = [
  ["passed", "go", "passaram"],
  ["asked", "ask", "viraram pergunta"],
  ["blocked", "stop", "barrados"],
  ["held", "stop", "segurados"],
];
const KIND_ICONS = { comando: iconCommand, arquivo: iconFile };
const clock = new Intl.DateTimeFormat("pt-BR", { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false });

let feed = { entries: [], exits: [], tally: { passed: 0, asked: 0, blocked: 0, held: 0 } };
const opened = new Set();
/** O projeto aberto: a portaria só mostra o que passou pelos chats dele. */
let scope = { projectId: null, chats: new Map(), openChat: () => {} };

function node(tag, className, text) {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (text !== undefined) element.textContent = text;
  return element;
}

function drawing(raw, className) {
  const holder = node("span", className);
  holder.innerHTML = raw;
  return holder;
}

function stamp(iso) {
  const at = new Date(iso);
  const time = node("time", "gate-stamp", Number.isNaN(at.valueOf()) ? "--:--:--" : clock.format(at));
  if (!Number.isNaN(at.valueOf())) time.dateTime = at.toISOString();
  return time;
}

function kindMark(icon, word) {
  const mark = node("span", "gate-kind");
  mark.append(drawing(icon, "gate-kind-icon"), node("span", null, word));
  return mark;
}

/** O caminho de volta: cada pedido analisado aponta o chat de onde veio. */
function chatRef(chatId) {
  const chat = scope.chats.get(chatId);
  const button = node("button", "gate-ref");
  button.type = "button";
  const code = chat?.code ? `#${chat.code}` : "chat encerrado";
  button.append(drawing(iconChatRef, "gate-kind-icon"), node("code", null, code), node("span", null, chat?.title ?? "não está mais no projeto"));
  if (!chat) { button.disabled = true; return button; }
  button.title = `Abrir o chat ${code} — ${chat.title}`;
  button.addEventListener("click", () => scope.openChat(chatId));
  return button;
}

function signal(raw, aspect, extra) {
  const head = drawing(raw, `gate-signal ${extra}`);
  head.dataset.aspect = aspect;
  return head;
}

function gauge(criterion) {
  const row = node("div", "gauge");
  if (criterion.band && !withinBand(criterion)) row.dataset.outside = "true";
  row.append(node("span", "gauge-label", criterion.label));

  const track = node("span", "gauge-track");
  if (criterion.band) {
    const band = node("span", "gauge-band");
    band.style.left = `${criterion.band[0]}%`;
    band.style.width = `${criterion.band[1] - criterion.band[0]}%`;
    track.append(band);
  }
  const fill = node("span", "gauge-fill");
  fill.style.width = `${criterion.percent}%`;
  const cap = node("span", "gauge-cap");
  cap.style.left = `${criterion.percent}%`;
  track.append(fill, cap);
  row.append(track);

  row.append(node("span", "gauge-value", `${criterion.percent}%`));
  if (criterion.id === "scope") {
    const scale = node("span", "gauge-scale");
    for (const level of ["ajuste pequeno", "funcionalidade", "sistema inteiro"]) {
      const tick = node("i", null, level);
      if (level === criterion.reading) tick.dataset.now = "true";
      scale.append(tick);
    }
    row.append(scale);
  } else {
    row.append(node("span", "gauge-reading", criterion.reading));
  }
  return row;
}

function withinBand(criterion) {
  return !criterion.band || (criterion.percent >= criterion.band[0] && criterion.percent <= criterion.band[1]);
}

function entryItem(check) {
  const { aspect, label } = ENTRY_VERDICTS[check.verdict] ?? ENTRY_VERDICTS.block;
  const item = node("article", "gate-item");
  item.dataset.aspect = aspect;
  const panelId = `criteria-${check.id}`;

  const head = node("button", "gate-face");
  head.type = "button";
  head.setAttribute("aria-expanded", String(opened.has(check.id)));
  head.setAttribute("aria-controls", panelId);
  head.append(signal(signalHead3, aspect, "gate-signal--three"));

  const body = node("span", "gate-body");
  const top = node("span", "gate-line");
  top.append(stamp(check.at), node("span", "gate-verdict", label));
  const score = node("span", "gate-score");
  score.append(node("b", null, String(check.score)), node("i", null, `mín. ${check.demand}`));
  top.append(score);

  body.append(top, node("p", "gate-prompt", check.prompt), node("p", "gate-note", check.note));
  head.append(body, drawing(iconChevron, "gate-chevron"));

  const panel = node("div", "gate-criteria");
  panel.id = panelId;
  panel.hidden = !opened.has(check.id);
  const heading = node("div", "gate-criteria-head");
  heading.append(node("span", null, `Um ${check.scope} precisa de ${check.demand} para atravessar sem ressalva.`));
  heading.append(node("span", "gate-source", `leitura: ${check.source}`));
  panel.append(heading);
  for (const criterion of check.criteria) panel.append(gauge(criterion));

  head.addEventListener("click", () => {
    const show = panel.hidden;
    panel.hidden = !show;
    head.setAttribute("aria-expanded", String(show));
    if (show) opened.add(check.id); else opened.delete(check.id);
  });

  const foot = node("div", "gate-foot");
  foot.append(chatRef(check.chatId));

  item.append(head, foot, panel);
  return item;
}

function exitItem(check) {
  const { aspect, label } = EXIT_VERDICTS[check.verdict] ?? EXIT_VERDICTS.held;
  const item = node("article", "gate-item");
  item.dataset.aspect = aspect;

  const face = node("div", "gate-face gate-face--fixed");
  face.append(signal(signalHead2, aspect, "gate-signal--two"));

  const body = node("span", "gate-body");
  const top = node("span", "gate-line");
  top.append(stamp(check.at), kindMark(KIND_ICONS[check.kind] ?? iconFile, check.kind), node("span", "gate-verdict", label));
  body.append(top, node("code", "gate-target", check.target));

  const rule = node("p", "gate-rule");
  rule.append(drawing(iconHouseRule, "gate-kind-icon"));
  rule.append(node("span", null, check.rule ?? "nenhuma regra da casa foi tocada"));
  if (!check.rule) rule.dataset.quiet = "true";
  body.append(rule);

  face.append(body);
  const foot = node("div", "gate-foot");
  foot.append(chatRef(check.chatId));
  item.append(face, foot);
  return item;
}

function renderLane(container, checks, build, empty) {
  container.replaceChildren();
  if (checks.length === 0) {
    container.append(node("p", "gate-empty", empty));
    return;
  }
  for (const check of checks) container.append(build(check));
}

function renderTally() {
  const board = document.querySelector("#gate-tally");
  board.replaceChildren();
  for (const [key, aspect, label] of TALLY) {
    const item = node("div", "tally-item");
    item.dataset.aspect = aspect;
    item.append(drawing(lamp, "tally-lamp"), node("b", null, String(feed.tally[key] ?? 0)), node("span", null, label));
    board.append(item);
  }
}

function render() {
  renderTally();
  document.querySelector("#entry-count").textContent = feed.entries.length ? `${feed.entries.length} no feed` : "";
  document.querySelector("#exit-count").textContent = feed.exits.length ? `${feed.exits.length} no feed` : "";
  renderLane(document.querySelector("#entry-lane"), feed.entries, entryItem, "Nenhum pedido chegou ainda neste projeto. Abra um chat e envie um prompt: ele é pontuado aqui antes de qualquer modelo ser chamado.");
  renderLane(document.querySelector("#exit-lane"), feed.exits, exitItem, "Nada saiu ainda neste projeto. Quando um modelo pedir para rodar um comando ou mexer num arquivo, o pedido aparece aqui com a regra que ele bateu.");
}

function prepend(lane, element) {
  const container = document.querySelector(lane);
  container.querySelector(".gate-empty")?.remove();
  element.dataset.arriving = "true";
  element.addEventListener("animationend", () => delete element.dataset.arriving, { once: true });
  container.prepend(element);
  container.scrollTop = 0;
}

/** Mostra que um pedido liberado está com o modelo, esperando a resposta. */
export function setGateBusy(busy) {
  document.querySelector("#exit-head")?.toggleAttribute("data-waiting", busy);
}

/** Diz à portaria de qual projeto ela está cuidando, e escreve isso no placar. */
export function setGateScope({ project, chats, openChat }) {
  scope = { projectId: project?.id ?? null, chats: new Map(chats.map((chat) => [chat.id, chat])), openChat };
  const plate = document.querySelector("#gate-project");
  plate.replaceChildren();
  if (!project) { plate.append(node("span", null, "dois portões, um feed cada")); return; }
  plate.append(node("b", null, project.name));
  if (project.rootPath) plate.append(drawing(iconFolder, "gate-kind-icon"), node("code", null, project.rootPath));
  else plate.append(node("i", null, "sem pasta no disco"));
}

/** O último registro da portaria naquele chat: a saída, quando houve alguma, ou
 * o pedido que entrou. É o que os cartões de chat mostram sem abrir a portaria.
 * A portaria vive na memória da sessão, então um chat antigo pode não ter nada. */
export function lastGatePass(chatId) {
  const exit = feed.exits.find((check) => check.chatId === chatId);
  const entry = feed.entries.find((check) => check.chatId === chatId);
  if (exit && (!entry || new Date(exit.at) >= new Date(entry.at))) {
    const { aspect, label } = EXIT_VERDICTS[exit.verdict] ?? EXIT_VERDICTS.held;
    return { aspect, label: `${exit.kind} ${label}`, detail: exit.target, at: exit.at };
  }
  if (!entry) return null;
  const { aspect, label } = ENTRY_VERDICTS[entry.verdict] ?? ENTRY_VERDICTS.block;
  return { aspect, label: `pedido ${label}`, detail: entry.prompt, at: entry.at };
}

function mine(chatId) {
  return scope.projectId !== null && scope.chats.has(chatId);
}

export async function loadGate() {
  try {
    feed = await invoke("gate_feed", { projectId: scope.projectId });
    render();
  } catch (error) {
    console.error(error);
  }
}

const VERDICT_TALLY = { pass: "passed", ask: "asked", block: "blocked" };

export function watchGate() {
  listen("gate-entry", ({ payload }) => {
    if (!mine(payload.check.chatId)) return;
    feed.entries.unshift(payload.check);
    feed.tally[VERDICT_TALLY[payload.check.verdict]] += 1;
    renderTally();
    document.querySelector("#entry-count").textContent = `${feed.entries.length} no feed`;
    prepend("#entry-lane", entryItem(payload.check));
  });
  listen("gate-exit", ({ payload }) => {
    const checks = payload.checks.filter((check) => mine(check.chatId));
    if (checks.length === 0) return;
    feed.exits.unshift(...checks);
    feed.tally.held += checks.filter((check) => check.verdict === "held").length;
    renderTally();
    document.querySelector("#exit-count").textContent = `${feed.exits.length} no feed`;
    for (const check of [...checks].reverse()) prepend("#exit-lane", exitItem(check));
  });
}

/** Coloca os desenhos fixos da tela: marca no menu, no placar e nas duas faixas. */
export function paintGateChrome() {
  document.querySelector("#nav-gate-mark").innerHTML = logo;
  document.querySelector("#gate-logo").innerHTML = logo;
  document.querySelector("#entry-head .gate-lane-icon").innerHTML = iconGateIn;
  document.querySelector("#exit-head .gate-lane-icon").innerHTML = iconGateOut;
}
