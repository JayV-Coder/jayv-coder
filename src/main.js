import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { loadGate, watchGate, paintGateChrome, setGateBusy, setGateScope, lastGatePass } from "./gate.js";
import iconFolder from "./assets/icon-folder.svg?raw";
import iconGrid from "./assets/icon-grid.svg?raw";
import iconList from "./assets/icon-list.svg?raw";

const $ = (selector, root = document) => root.querySelector(selector);
const $$ = (selector, root = document) => [...root.querySelectorAll(selector)];
const state = { workspace: { projects: [], chats: [] }, activeProjectId: null, activeChatId: null, view: "projects", layout: localStorage.getItem("jev.layout") === "list" ? "list" : "grid", settings: null, discovered: new Map() };
const since = new Intl.DateTimeFormat("pt-BR", { day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit" });
const timeline = $("#timeline");
const form = $("#prompt-form");
const prompt = $("#prompt");

function escapeHtml(value) {
  return String(value ?? "").replace(/[&<>'"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[char]);
}

function addMessage(role, content, meta = "") {
  const article = document.createElement("article");
  article.className = `message ${role}`;
  const head=document.createElement("div");
  head.className="message-head";
  const author=document.createElement("strong"); author.textContent=role === "user" ? "Você" : "Jev";
  const metadata=document.createElement("small"); metadata.textContent=meta;
  head.append(author,metadata);
  const body=document.createElement("div"); body.className="message-body";
  if (role === "assistant") renderAssistantContent(body,content); else body.textContent=content;
  article.append(head,body);
  timeline.append(article);
  timeline.scrollTop = timeline.scrollHeight;
  return article;
}

function renderAssistantContent(container, content) {
  const fence=/```([^\n`]*)\n?([\s\S]*?)```/g;
  let cursor=0; let match;
  while ((match=fence.exec(content)) !== null) {
    renderMarkdownText(container,content.slice(cursor,match.index));
    const block=document.createElement("section"); block.className="response-code";
    const header=document.createElement("div"); header.className="response-code-head";
    const language=document.createElement("span"); language.textContent=match[1].trim() || "código";
    const copy=document.createElement("button"); copy.type="button"; copy.className="copy-code"; copy.textContent="Copiar"; copy.dataset.code=match[2].replace(/\n$/,"");
    const pre=document.createElement("pre"); const code=document.createElement("code"); code.textContent=copy.dataset.code; pre.append(code);
    header.append(language,copy); block.append(header,pre); container.append(block);
    cursor=fence.lastIndex;
  }
  renderMarkdownText(container,content.slice(cursor));
}

function renderMarkdownText(container, value) {
  const lines=value.replace(/^\n+|\n+$/g,"").split("\n");
  for (let index=0; index<lines.length;) {
    const line=lines[index];
    if (!line.trim()) {index+=1; continue;}
    const heading=line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) {const node=document.createElement(`h${Math.min(heading[1].length+2,6)}`); appendInline(node,heading[2]); container.append(node); index+=1; continue;}
    if (line.includes("|") && lines[index+1]?.match(/^\s*\|?\s*:?-+/)) {index=renderTable(container,lines,index); continue;}
    const unordered=line.match(/^\s*[-*]\s+(.+)$/); const ordered=line.match(/^\s*\d+[.)]\s+(.+)$/);
    if (unordered || ordered) {
      const list=document.createElement(ordered ? "ol" : "ul"); const pattern=ordered ? /^\s*\d+[.)]\s+(.+)$/ : /^\s*[-*]\s+(.+)$/;
      while(index<lines.length){const item=lines[index].match(pattern);if(!item)break;const li=document.createElement("li");appendInline(li,item[1]);list.append(li);index+=1;}
      container.append(list); continue;
    }
    if (line.match(/^>\s?/)) {const quote=document.createElement("blockquote"); const parts=[]; while(index<lines.length&&lines[index].match(/^>\s?/)){parts.push(lines[index].replace(/^>\s?/,""));index+=1;} appendInline(quote,parts.join("\n"));container.append(quote);continue;}
    if (/^\s*---+\s*$/.test(line)){container.append(document.createElement("hr"));index+=1;continue;}
    const paragraph=[];
    while(index<lines.length && lines[index].trim() && !/^(#{1,4})\s+/.test(lines[index]) && !/^\s*(?:[-*]|\d+[.)])\s+/.test(lines[index]) && !/^>\s?/.test(lines[index])) {paragraph.push(lines[index]);index+=1;}
    const node=document.createElement("p"); appendInline(node,paragraph.join("\n")); container.append(node);
  }
}

function appendInline(parent, text) {
  const pattern=/(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\(https?:\/\/[^)\s]+\))/g; let cursor=0; let match;
  while((match=pattern.exec(text)) !== null){parent.append(document.createTextNode(text.slice(cursor,match.index)));const token=match[0];
    if(token.startsWith("**")){const strong=document.createElement("strong");strong.textContent=token.slice(2,-2);parent.append(strong);}
    else if(token.startsWith("`")){const code=document.createElement("code");code.textContent=token.slice(1,-1);parent.append(code);}
    else {const linkMatch=token.match(/^\[([^\]]+)\]\((https?:\/\/[^)]+)\)$/);const link=document.createElement("a");link.textContent=linkMatch[1];link.href=linkMatch[2];link.target="_blank";link.rel="noreferrer";parent.append(link);}
    cursor=pattern.lastIndex;
  }
  parent.append(document.createTextNode(text.slice(cursor)));
}

function renderTable(container, lines, start) {
  const cells=(line) => line.replace(/^\s*\||\|\s*$/g,"").split("|").map((cell) => cell.trim());
  const table=document.createElement("table"); const head=document.createElement("thead"); const headRow=document.createElement("tr");
  cells(lines[start]).forEach((value)=>{const th=document.createElement("th");appendInline(th,value);headRow.append(th);}); head.append(headRow); table.append(head);
  const body=document.createElement("tbody"); let index=start+2;
  while(index<lines.length && lines[index].includes("|")){const row=document.createElement("tr");cells(lines[index]).forEach((value)=>{const td=document.createElement("td");appendInline(td,value);row.append(td);});body.append(row);index+=1;}
  table.append(body); const wrapper=document.createElement("div");wrapper.className="response-table";wrapper.append(table);container.append(wrapper);return index;
}

function activeChat() { return state.workspace.chats.find((chat) => chat.id === state.activeChatId); }

function renderTimeline() {
  timeline.innerHTML = "";
  const chat = activeChat();
  const project = activeProject();
  if (!chat || chat.messages.length === 0) {
    timeline.innerHTML = project
      ? `<article class="welcome"><p class="eyebrow">${escapeHtml(project.name.toUpperCase())}</p><h2>O que vamos construir?</h2><p>Cada conversa guarda o próprio contexto dentro do projeto e ganha um identificador — é por ele que a Portaria aponta de volta para aqui.</p>${chat ? "" : '<button class="primary welcome-chat" type="button">Novo chat</button>'}</article>`
      : '<article class="welcome"><p class="eyebrow">JEV DESKTOP</p><h2>Abra um projeto</h2><p>Os projetos organizam seus chats, sua portaria e a pasta de trabalho no disco.</p><button class="primary welcome-projects" type="button">Ver projetos</button></article>';
  } else {
    chat.messages.forEach((message) => addMessage(message.role, message.content));
  }
  $("#view-title").textContent = chat?.title ?? "Selecione um chat";
  const badge = $("#chat-code");
  badge.textContent = chat ? `#${chat.code}` : "";
  badge.hidden = !chat || !$("#chat-view").classList.contains("active");
  $("#clear").hidden = !chat;
  prompt.disabled = !chat;
  $("#send").disabled = !chat;
}

function activeProject() { return state.workspace.projects.find((project) => project.id === state.activeProjectId); }

function chatsOf(projectId) {
  return state.workspace.chats.filter((chat) => chat.projectId === projectId).sort((a, b) => new Date(b.updatedAt) - new Date(a.updatedAt));
}

const RECENT_CHATS = 3;

/** O chat só aparece selecionado enquanto a conversa dele está na tela: na
 * grade de chats nenhuma linha fica marcada. */
function chatIsOpen(chat) { return chat.id === state.activeChatId && state.view === "chat"; }

function chatRow(chat) {
  const open = chatIsOpen(chat);
  return `<div class="chat-row"><button class="chat-item ${open ? "active" : ""}" ${open ? 'aria-current="true"' : ""} data-chat-id="${escapeHtml(chat.id)}" title="${escapeHtml(chat.title)} · #${escapeHtml(chat.code)}${open ? " · aberto agora" : ""}"><span class="chat-dot">${open ? "●" : "◌"}</span><span class="chat-title">${escapeHtml(chat.title)}</span><code class="chat-tag">#${escapeHtml(chat.code)}</code></button><button class="delete-chat" data-delete-chat-id="${escapeHtml(chat.id)}" data-chat-title="${escapeHtml(chat.title)}" title="Excluir chat" aria-label="Excluir chat ${escapeHtml(chat.title)}">×</button></div>`;
}

/** Os três chats mais recentes do projeto. O chat aberto entra na lista mesmo
 * quando é antigo: é nela que se vê qual conversa está na tela, e a grade de
 * chats continua mostrando o projeto inteiro. */
function recentChats(projectId) {
  const chats = chatsOf(projectId);
  const recent = chats.slice(0, RECENT_CHATS);
  const open = chats.find(chatIsOpen);
  if (!open || recent.includes(open)) return recent;
  return [...recent.slice(0, RECENT_CHATS - 1), open];
}

/** A lateral só mostra o projeto aberto: a placa dele, a portaria e os chats. */
function renderSidebar() {
  const project = activeProject();
  $("#main-nav").hidden = Boolean(project);
  $("#project-context").hidden = !project;
  if (!project) { $("#chat-list").innerHTML = ""; return; }
  $("#plate-name").textContent = project.name;
  $("#plate-name").title = project.name;
  const path = $("#plate-path");
  path.textContent = project.rootPath || "sem pasta no disco";
  path.title = project.rootPath || "Nenhuma pasta foi escolhida quando este projeto foi criado.";
  path.toggleAttribute("data-empty", !project.rootPath);
  $("#chat-list").innerHTML = recentChats(project.id).map(chatRow).join("") || '<p class="empty-list">Nenhum chat</p>';
}

function shorten(text, limit) {
  const clean = String(text ?? "").replace(/\s+/g, " ").trim();
  return clean.length > limit ? `${clean.slice(0, limit - 1)}…` : clean;
}

/** O cartão de um chat: como ele se chama, quando nasceu, o que a portaria
 * registrou nele por último e o pedido mais recente que ele recebeu. */
function chatCard(chat) {
  const pass = lastGatePass(chat.id);
  const said = [...chat.messages].reverse().find((message) => message.role === "user")?.content;
  const gate = pass
    ? `<span class="chat-gate" data-aspect="${escapeHtml(pass.aspect)}"><b>${escapeHtml(pass.label)}</b><code>${escapeHtml(shorten(pass.detail, 64))}</code></span>`
    : '<span class="chat-gate" data-empty="true">a portaria não registrou nada nesta sessão</span>';
  const last = said
    ? `<span class="chat-said">${escapeHtml(shorten(said, 150))}</span>`
    : '<span class="chat-said" data-empty="true">nenhum pedido enviado ainda</span>';
  return `<article class="yard-card chat-card">
    <button class="yard-open" type="button" data-chat-id="${escapeHtml(chat.id)}">
      <span class="yard-name">${escapeHtml(chat.title)}</span>
      <span class="chat-meta"><code>#${escapeHtml(chat.code)}</code><i>criado em ${escapeHtml(since.format(new Date(chat.createdAt)))}</i></span>
      ${gate}
      ${last}
    </button>
    <div class="yard-actions">
      <button class="yard-action yard-danger" type="button" data-delete-chat-id="${escapeHtml(chat.id)}" data-chat-title="${escapeHtml(chat.title)}">Excluir</button>
    </div>
  </article>`;
}

/** A grade de chats é a primeira tela de um projeto aberto. */
function renderChats() {
  const project = activeProject();
  $("#chats-eyebrow").textContent = project ? `PROJETO · ${project.name.toUpperCase()}` : "PROJETO";
  $("#chat-grid").innerHTML = chatsOf(state.activeProjectId).map(chatCard).join("")
    || '<p class="yard-empty">Nenhum chat neste projeto ainda. Crie o primeiro e o pedido já passa pela portaria.</p>';
}

/** O cartão mostra o registro da portaria, que mora no núcleo: busca o feed do
 * projeto antes de desenhar. */
async function loadChats() {
  await loadGate();
  renderChats();
}

function projectCard(project) {
  const chats = chatsOf(project.id);
  const last = chats[0]?.updatedAt ?? project.createdAt;
  const path = project.rootPath
    ? `<span class="yard-path" title="${escapeHtml(project.rootPath)}">${iconFolder}<code>${escapeHtml(project.rootPath)}</code></span>`
    : '<span class="yard-path" data-empty="true">sem pasta no disco</span>';
  return `<article class="yard-card">
    <button class="yard-open" type="button" data-open-project="${escapeHtml(project.id)}">
      <span class="yard-name">${escapeHtml(project.name)}</span>
      ${path}
      <span class="yard-stats"><b>${chats.length}</b> ${chats.length === 1 ? "chat" : "chats"}<i>último movimento ${escapeHtml(since.format(new Date(last)))}</i></span>
    </button>
    <div class="yard-actions">
      <button class="yard-action" type="button" data-new-chat-project="${escapeHtml(project.id)}">Novo chat</button>
      <button class="yard-action yard-danger" type="button" data-delete-project="${escapeHtml(project.id)}" data-project-name="${escapeHtml(project.name)}">Excluir</button>
    </div>
  </article>`;
}

function renderYard() {
  $("#project-grid").innerHTML = state.workspace.projects.map(projectCard).join("")
    || '<p class="yard-empty">Nenhum projeto ainda. Crie o primeiro para abrir a portaria dele.</p>';
}

function setLayout(layout) {
  state.layout = layout;
  localStorage.setItem("jev.layout", layout);
  $("#project-grid").dataset.layout = layout;
  $$(".switch-option").forEach((button) => button.setAttribute("aria-pressed", String(button.dataset.layout === layout)));
}

function syncGateScope() {
  setGateScope({ project: activeProject() ?? null, chats: chatsOf(state.activeProjectId), openChat });
}

/** Entra no projeto: a grade de chats dele é a primeira coisa que aparece. */
function openProject(projectId) {
  state.activeProjectId = projectId;
  const chats = chatsOf(projectId);
  if (!chats.some((chat) => chat.id === state.activeChatId)) state.activeChatId = chats[0]?.id ?? null;
  syncGateScope();
  renderSidebar();
  renderTimeline();
  navigate("chats");
}

function openChat(chatId) {
  const chat = state.workspace.chats.find((item) => item.id === chatId);
  if (!chat) return;
  state.activeProjectId = chat.projectId;
  state.activeChatId = chatId;
  syncGateScope();
  renderSidebar();
  renderTimeline();
  navigate("chat");
}

function leaveProject() {
  state.activeProjectId = null;
  state.activeChatId = null;
  syncGateScope();
  renderSidebar();
  renderTimeline();
  navigate("projects");
}

async function loadWorkspace(preferredChatId) {
  state.workspace = await invoke("get_workspace");
  if (!state.workspace.projects.some((project) => project.id === state.activeProjectId)) state.activeProjectId = null;
  const preferred = state.workspace.chats.find((chat) => chat.id === preferredChatId);
  if (preferred) { state.activeProjectId = preferred.projectId; state.activeChatId = preferred.id; }
  else if (!chatsOf(state.activeProjectId).some((chat) => chat.id === state.activeChatId)) state.activeChatId = chatsOf(state.activeProjectId)[0]?.id ?? null;
  syncGateScope();
  renderSidebar();
  renderYard();
  renderChats();
  renderTimeline();
}

/** O núcleo troca o título do chat depois da primeira resposta: a lateral, o
 * cabeçalho e as referências da portaria passam a mostrar o nome novo sem
 * recarregar a conversa. */
function watchRenames() {
  listen("chat-renamed", ({ payload }) => {
    const chat = state.workspace.chats.find((item) => item.id === payload.chatId);
    if (!chat) return;
    chat.title = payload.title;
    if (chat.id === state.activeChatId) $("#view-title").textContent = chat.title;
    syncGateScope();
    renderSidebar();
    renderYard();
    renderChats();
  });
}

function databaseCard(status) {
  const rows = (status.tables || []).map((table) => `<tr><th scope="row">${escapeHtml(table.name)}</th><td>${escapeHtml(table.rows)}</td></tr>`).join("")
    || '<tr><td colspan="2">Nenhuma tabela ainda.</td></tr>';
  return `<div class="metric database-card"><small>Banco local</small><strong title="${escapeHtml(status.database_path)}">${escapeHtml(status.database_name)}</strong><table class="table-counts"><thead><tr><th scope="col">Tabela</th><th scope="col">Registros</th></tr></thead><tbody>${rows}</tbody></table></div>`;
}

async function loadStatus() {
  try {
    const status = await invoke("system_status");
    const entries = [
      ["Provedores prontos", status.providers], ["Modelos ativos", status.models], ["Arquivos indexados", status.indexed_files],
      ["Cache", `${status.cache_entries} entradas`], ["Histórico", `${status.session_messages} mensagens`], ["Versão", status.version],
    ];
    $("#status-grid").innerHTML = entries.map(([label, value]) => `<div class="metric"><small>${label}</small><strong>${escapeHtml(value)}</strong></div>`).join("") + databaseCard(status);
  } catch (error) {
    showFeedback(String(error), true);
  }
}

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const value = prompt.value.trim();
  const chatId = state.activeChatId;
  if (!value || !chatId) return;
  addMessage("user", value);
  prompt.value = "";
  prompt.style.height = "auto";
  $("#send").disabled = true;
  setGateBusy(true);
  const pending = addMessage("assistant", "Analisando intenção, contexto e rota…", "em andamento");
  try {
    const result = await invoke("process_request", { request: { input: value, sessionId: chatId } });
    pending.remove();
    const response = result.result?.response ?? result.error ?? "A execução terminou sem resposta.";
    const meta = result.error ? "erro" : `${result.decision.model_provider} · ${result.decision.model_name} · ${result.complexity}`;
    addMessage("assistant", response, meta);
    await loadWorkspace(chatId);
  } catch (error) {
    pending.remove();
    addMessage("assistant", String(error), "erro");
  } finally {
    setGateBusy(false);
    $("#send").disabled = false;
    prompt.focus();
    loadStatus();
  }
});

prompt.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    form.requestSubmit();
  }
});

prompt.addEventListener("input", () => {
  prompt.style.height = "auto";
  prompt.style.height = `${Math.min(prompt.scrollHeight, 180)}px`;
});

function navigate(view) {
  state.view = view;
  $$(".nav, .view").forEach((element) => element.classList.remove("active"));
  $(`.nav[data-view="${view}"]`)?.classList.add("active");
  $(`#${view}-view`).classList.add("active");
  const chat = activeChat();
  $("#clear").hidden = view !== "chat" || !chat;
  $("#chat-code").hidden = view !== "chat" || !chat;
  $("main > header").hidden = view === "gate" || view === "projects" || view === "chats";
  $("#view-eyebrow").textContent = view === "chat" ? `PROJETO · ${activeProject()?.name ?? ""}` : view === "status" ? "OBSERVABILIDADE" : "PREFERÊNCIAS";
  if (view === "chat") $("#view-title").textContent = chat?.title ?? "Chats";
  if (view === "status") { $("#view-title").textContent = "Sistema"; loadStatus(); }
  if (view === "settings") { $("#view-title").textContent = "Configuração"; loadSettings(); }
  if (view === "chats") loadChats();
  if (view === "gate") loadGate();
  renderSidebar();
}

$$(".nav").forEach((button) => button.addEventListener("click", () => navigate(button.dataset.view)));

async function deleteChat(chatId, title) {
  if (!window.confirm(`Excluir o chat “${title}”? Esta ação remove todo o histórico.`)) return;
  try {await invoke("delete_chat",{chatId});if(state.activeChatId===chatId)state.activeChatId=null;await loadWorkspace();}
  catch(error){showFeedback(String(error),true);}
}

/** A lista da lateral e a grade de chats respondem aos mesmos botões. */
async function onChatClick(event) {
  const remove=event.target.closest("[data-delete-chat-id]");
  if (remove) { await deleteChat(remove.dataset.deleteChatId,remove.dataset.chatTitle); return; }
  const chatButton = event.target.closest("[data-chat-id]");
  if (chatButton) openChat(chatButton.dataset.chatId);
}

$("#chat-list").addEventListener("click", onChatClick);
$("#chat-grid").addEventListener("click", onChatClick);
$("#new-chat-card").addEventListener("click", () => { if (state.activeProjectId) createChat(state.activeProjectId); });

async function createChat(projectId) {
  try { const chat = await invoke("create_chat", { projectId, title: null }); await loadWorkspace(chat.id); navigate("chat"); prompt.focus(); }
  catch (error) { showFeedback(String(error), true); }
}

async function deleteProject(projectId, name) {
  const chatCount = state.workspace.chats.filter((chat) => chat.projectId === projectId).length;
  if (!window.confirm(`Excluir o projeto “${name}” e ${chatCount} chat${chatCount === 1 ? "" : "s"}? Esta ação remove todo o histórico associado.`)) return;
  try { if (state.activeProjectId === projectId) { state.activeProjectId = null; state.activeChatId = null; } await invoke("delete_project", { projectId }); await loadWorkspace(); navigate("projects"); }
  catch (error) { showFeedback(String(error), true); }
}

$("#project-grid").addEventListener("click", async (event) => {
  const remove = event.target.closest("[data-delete-project]");
  if (remove) { await deleteProject(remove.dataset.deleteProject, remove.dataset.projectName); return; }
  const fresh = event.target.closest("[data-new-chat-project]");
  if (fresh) { state.activeProjectId = fresh.dataset.newChatProject; await createChat(fresh.dataset.newChatProject); return; }
  const open = event.target.closest("[data-open-project]");
  if (open) openProject(open.dataset.openProject);
});

$$(".switch-option").forEach((button) => button.addEventListener("click", () => setLayout(button.dataset.layout)));
$("#leave-project").addEventListener("click", leaveProject);
$("#new-chat").addEventListener("click", () => { if (state.activeProjectId) createChat(state.activeProjectId); });
$("#nav-gate").addEventListener("click", () => navigate("gate"));

timeline.addEventListener("click",async(event)=>{
  const copy=event.target.closest(".copy-code");
  if(copy){await navigator.clipboard.writeText(copy.dataset.code);copy.textContent="Copiado";window.setTimeout(()=>{copy.textContent="Copiar";},1500);return;}
  if(event.target.closest(".welcome-chat") && state.activeProjectId) {createChat(state.activeProjectId);return;}
  if(event.target.closest(".welcome-projects")) navigate("projects");
});

$("#new-project").addEventListener("click", () => { $("#project-form").reset(); $("#path-note").textContent=""; delete $("#project-name").dataset.typed; $("#project-dialog").showModal(); $("#project-name").focus(); });

/** O nome que a pasta escolhida sugere: o último trecho do caminho, com barra
 * do Windows ou do Unix. */
function folderName(path) { return path.split(/[\\/]+/).filter(Boolean).pop() ?? ""; }

/** Quem digita o nome manda: a pasta só preenche o campo enquanto ele estiver
 * intocado ou vazio. */
$("#project-name").addEventListener("input", (event) => {
  if (event.target.value.trim()) event.target.dataset.typed = "true"; else delete event.target.dataset.typed;
});

$("#pick-path").addEventListener("click", async () => {
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const chosen = await open({ directory: true, multiple: false, title: "Escolha a pasta do projeto" });
    if (typeof chosen === "string") {
      $("#project-path").value = chosen;
      $("#path-note").textContent = "";
      const name = $("#project-name");
      if (!name.dataset.typed) name.value = folderName(chosen);
    }
  } catch (error) {
    $("#path-note").textContent = "O seletor de pastas não abriu. Escreva o caminho completo aqui.";
    console.error(error);
  }
});
$("#cancel-project").addEventListener("click", () => $("#project-dialog").close());
$("#project-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const project = await invoke("create_project", { name: $("#project-name").value, rootPath: $("#project-path").value.trim() || null });
    const chat = await invoke("create_chat", { projectId: project.id, title: null });
    $("#project-dialog").close();
    await loadWorkspace(chat.id);
    navigate("chat");
    prompt.focus();
  } catch (error) { showFeedback(String(error), true); }
});

$("#clear").addEventListener("click", async () => {
  if (!state.activeChatId) return;
  await invoke("clear_chat", { chatId: state.activeChatId });
  await loadWorkspace(state.activeChatId);
  loadStatus();
});

function providerCard(provider) {
  const keyHint = provider.hasApiKey ? "Chave configurada — deixe vazio para manter" : "Insira a API key";
  return `<article class="config-card provider-card" data-original-name="${escapeHtml(provider.name)}">
    <div class="config-card-head"><label class="toggle"><input data-field="enabled" type="checkbox" ${provider.enabled ? "checked" : ""}/><span></span></label><input class="title-input" data-field="name" value="${escapeHtml(provider.name)}" aria-label="Nome do provedor"/><button class="icon-button remove-card" title="Remover provedor">×</button></div>
    <div class="form-grid">
      <label>Tipo<select data-field="kind"><option value="openai">OpenAI</option><option value="anthropic">Anthropic</option><option value="openai-compatible">OpenAI compatível</option><option value="cli">CLI local</option></select></label>
      <label>Timeout (s)<input data-field="timeout" type="number" min="5" value="${provider.timeout || 30}"/></label>
      <label class="wide api-field">API key<input data-field="apiKey" type="password" autocomplete="new-password" placeholder="${escapeHtml(keyHint)}"/></label>
      <label class="wide base-field">URL base<input data-field="baseUrl" value="${escapeHtml(provider.baseUrl)}" placeholder="https://api.exemplo.com/v1"/></label>
      <label class="wide cli-field">Comando local<input data-field="command" value="${escapeHtml(provider.command)}" placeholder="claude"/></label>
      <label class="wide cli-field">Argumentos<input data-field="args" value="${escapeHtml((provider.args || []).join(" "))}" placeholder="--model {model} --print"/></label>
    </div>
    <label class="clear-key ${provider.hasApiKey ? "" : "hidden"}"><input data-field="clearApiKey" type="checkbox"/> Remover a chave salva</label>
    <div class="catalog-actions"><button class="ghost discover-models" type="button">Carregar modelos disponíveis</button><span class="catalog-status"></span></div>
    <div class="model-catalog"></div>
  </article>`;
}

function modelCard(model) {
  const providerNames=$$(".provider-card [data-field=name]").map((input) => input.value.trim()).filter(Boolean);
  const names=providerNames.length ? providerNames : (state.settings?.providers || []).map((provider) => provider.name);
  const options=names.map((name) => `<option value="${escapeHtml(name)}" ${name === model.provider ? "selected" : ""}>${escapeHtml(name)}</option>`).join("");
  return `<article class="config-card model-card">
    <div class="config-card-head"><label class="toggle"><input data-field="enabled" type="checkbox" ${model.enabled ? "checked" : ""}/><span></span></label><input class="title-input" data-field="name" value="${escapeHtml(model.name)}" aria-label="Nome da configuração do modelo"/><button class="icon-button remove-card" title="Remover modelo">×</button></div>
    <div class="form-grid model-grid"><label>Provedor<select data-field="provider">${options}</select></label><label>ID do modelo<input data-field="model" value="${escapeHtml(model.model)}"/></label><label class="wide">Capacidades<input data-field="capabilities" value="${escapeHtml((model.capabilities || []).join(", "))}" placeholder="chat, code, reasoning, tools"/></label><label>Janela de contexto<input data-field="contextWindow" type="number" min="1024" value="${model.contextWindow || 8192}"/></label><label>Custo<select data-field="costClass"><option value="free">Grátis/local</option><option value="low">Baixo</option><option value="medium">Médio</option><option value="high">Alto</option></select></label><label>Velocidade<select data-field="speed"><option value="fast">Rápida</option><option value="medium">Média</option><option value="slow">Lenta</option></select></label></div>
  </article>`;
}

function syncSelects() {
  $$(".provider-card").forEach((card, index) => { $("[data-field=kind]", card).value = state.settings.providers[index]?.kind || "openai"; updateProviderFields(card); });
  $$(".model-card").forEach((card, index) => { $("[data-field=costClass]", card).value = state.settings.models[index]?.costClass || "medium"; $("[data-field=speed]", card).value = state.settings.models[index]?.speed || "medium"; });
}

function renderSettings() {
  $("#config-path").textContent = state.settings.configPath;
  $("#provider-list").innerHTML = state.settings.providers.map(providerCard).join("") || '<p class="empty-state">Nenhum provedor configurado.</p>';
  $("#model-list").innerHTML = state.settings.models.map(modelCard).join("") || '<p class="empty-state">Nenhum modelo configurado.</p>';
  syncSelects();
}

async function loadSettings() {
  try { state.settings = await invoke("get_settings"); renderSettings(); }
  catch (error) { showFeedback(String(error), true); }
}

function updateProviderFields(card) {
  const kind=$("[data-field=kind]", card).value;
  card.classList.toggle("is-cli", kind === "cli");
  card.classList.toggle("is-compatible", kind === "openai-compatible");
}

function readProvider(card) {
  return { name: $("[data-field=name]",card).value.trim(), originalName: card.dataset.originalName || null, enabled: $("[data-field=enabled]",card).checked, kind: $("[data-field=kind]",card).value, apiKey: $("[data-field=apiKey]",card).value, clearApiKey: $("[data-field=clearApiKey]",card)?.checked || false, baseUrl: $("[data-field=baseUrl]",card).value, command: $("[data-field=command]",card).value, timeout: Number($("[data-field=timeout]",card).value) || 30, args: $("[data-field=args]",card).value.trim().split(/\s+/).filter(Boolean) };
}

function readModel(card) {
  return { name: $("[data-field=name]",card).value.trim(), enabled: $("[data-field=enabled]",card).checked, provider: $("[data-field=provider]",card).value, model: $("[data-field=model]",card).value.trim(), capabilities: $("[data-field=capabilities]",card).value.split(",").map((item) => item.trim()).filter(Boolean), costClass: $("[data-field=costClass]",card).value, speed: $("[data-field=speed]",card).value, contextWindow: Number($("[data-field=contextWindow]",card).value) || 8192 };
}

async function discoverModels(card, automatic = false) {
  const provider=readProvider(card);
  if (provider.kind === "cli") return;
  const status=$(".catalog-status",card);
  status.textContent="Consultando catálogo…";
  try {
    const models=await invoke("discover_provider_models",{provider});
    state.discovered.set(provider.name,models);
    status.textContent=`${models.length} modelos encontrados`;
    $(".model-catalog",card).innerHTML=models.map((model) => `<button type="button" class="model-pill" data-add-model="${escapeHtml(model)}" data-provider="${escapeHtml(provider.name)}">+ ${escapeHtml(model)}</button>`).join("");
  } catch (error) {
    status.textContent=automatic ? "" : String(error);
    if (!automatic) showFeedback(String(error),true);
  }
}

$("#provider-list").addEventListener("change", (event) => {
  const card=event.target.closest(".provider-card");
  if (event.target.matches("[data-field=kind]")) updateProviderFields(card);
});

$("#provider-list").addEventListener("input", (event) => {
  if (!event.target.matches("[data-field=name]")) return;
  const card=event.target.closest(".provider-card");
  const oldName=card.dataset.lastName || card.dataset.originalName;
  const newName=event.target.value.trim();
  $$(".model-card [data-field=provider]").forEach((select) => {
    const selected=select.value;
    const names=$$(".provider-card [data-field=name]").map((input) => input.value.trim()).filter(Boolean);
    select.innerHTML=names.map((name) => `<option value="${escapeHtml(name)}">${escapeHtml(name)}</option>`).join("");
    select.value=selected === oldName ? newName : selected;
  });
  card.dataset.lastName=newName;
});

$("#provider-list").addEventListener("focusout", (event) => {
  if (!event.target.matches("[data-field=apiKey], [data-field=baseUrl]")) return;
  const card=event.target.closest(".provider-card");
  const provider=readProvider(card);
  if ((provider.apiKey || card.dataset.originalName) && (provider.kind !== "openai-compatible" || provider.baseUrl)) discoverModels(card,true);
});

$("#provider-list").addEventListener("click", (event) => {
  const card=event.target.closest(".provider-card");
  if (event.target.closest(".remove-card")) { card.remove(); return; }
  if (event.target.closest(".discover-models")) { discoverModels(card); return; }
  const pill=event.target.closest("[data-add-model]");
  if (pill) addModel({name:uniqueModelName(pill.dataset.addModel),enabled:true,provider:pill.dataset.provider,model:pill.dataset.addModel,capabilities:["chat","code","reasoning","tools"],costClass:"medium",speed:"medium",contextWindow:128000});
});

$("#model-list").addEventListener("click", (event) => { if (event.target.closest(".remove-card")) event.target.closest(".model-card").remove(); });

function uniqueModelName(modelId) {
  const base=modelId.toLowerCase().replace(/[^a-z0-9]+/g,"-").replace(/^-|-$/g,"").slice(0,48) || "model";
  const names=new Set($$(".model-card").map((card) => $("[data-field=name]",card).value));
  let name=base; let counter=2; while(names.has(name)) name=`${base}-${counter++}`; return name;
}

function addModel(model) {
  $("#model-list .empty-state")?.remove();
  const wrapper=document.createElement("div"); wrapper.innerHTML=modelCard(model); $("#model-list").append(wrapper.firstElementChild);
  const card=$("#model-list .model-card:last-child"); $("[data-field=costClass]",card).value=model.costClass; $("[data-field=speed]",card).value=model.speed; card.scrollIntoView({behavior:"smooth",block:"center"});
}

$("#add-provider").addEventListener("click", () => {
  $("#provider-list .empty-state")?.remove();
  const wrapper=document.createElement("div"); wrapper.innerHTML=providerCard({name:`provider-${$$('.provider-card').length+1}`,enabled:true,kind:"openai",hasApiKey:false,baseUrl:"",command:"",timeout:30,args:[]}); $("#provider-list").append(wrapper.firstElementChild); updateProviderFields($("#provider-list .provider-card:last-child"));
});

$("#add-model").addEventListener("click", () => addModel({name:uniqueModelName("novo-modelo"),enabled:true,provider:$$('.provider-card [data-field=name]')[0]?.value || "",model:"",capabilities:["chat"],costClass:"medium",speed:"medium",contextWindow:8192}));

$("#save-settings").addEventListener("click", async () => {
  const settings={providers:$$('.provider-card').map(readProvider),models:$$('.model-card').map(readModel)};
  $("#save-settings").disabled=true;
  try { state.settings=await invoke("save_settings",{settings}); renderSettings(); showFeedback("Configuração salva e provedores recarregados."); loadStatus(); }
  catch (error) { showFeedback(String(error),true); }
  finally { $("#save-settings").disabled=false; }
});

function showFeedback(message, error = false) {
  const feedback=$("#settings-feedback");
  feedback.hidden=false; feedback.textContent=message; feedback.classList.toggle("error",error);
  window.clearTimeout(showFeedback.timeout); showFeedback.timeout=window.setTimeout(() => { feedback.hidden=true; },6000);
}

paintGateChrome();
$("#icon-grid").innerHTML=iconGrid;
$("#icon-list").innerHTML=iconList;
setLayout(state.layout);
watchGate();
watchRenames();
navigate("projects");
Promise.all([loadWorkspace(),loadStatus()]).catch((error) => { showFeedback(String(error),true); console.error(error); });
