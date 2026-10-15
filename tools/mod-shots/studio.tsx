// A entrada do estúdio: lê o idioma e o tema do endereço, pede os dados de
// exemplo ao servidor do Vite (`/@studio/data`) e abre o app de verdade por
// cima do núcleo de mentira. `window.__studio` é por onde o `shoot.mjs` monta
// cada cena (abrir um chat, uma aba das configurações…).
import "@fontsource-variable/inter";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "@fontsource/ibm-plex-mono/600.css";
import "@/styles/globals.css";
import type { StudioData } from "./fake/fixtures";

const params = new URLSearchParams(location.search);
const locale = params.get("locale") ?? "en";
const theme = params.get("theme") === "dark" ? "dark" : "light";

// O que a pessoa já teria visto: sem tutorial nem "o que há de novo" na frente.
localStorage.clear();
localStorage.setItem("jayv.locale", locale);
localStorage.setItem("jayv.theme", theme);
localStorage.setItem("jayv.tutorial", JSON.stringify({ seen: [], auto: false }));
localStorage.setItem("jayv.whatsNew.seen", "999.0.0");
localStorage.setItem("jayv.progress.open", "1");
localStorage.setItem("jayv.progress.width", "300");
localStorage.setItem("jayv.layout", "list");

const data = (await (await fetch(`/@studio/data?locale=${encodeURIComponent(locale)}`)).json()) as StudioData;
window.__studioData = data;

const [{ createRoot }, { App }, { connectTheme }, navigation, workspace, settings, live, conversation, emits] = await Promise.all([
  import("react-dom/client"), import("@/app/App"), import("@/modules/theme"), import("@/modules/navigation"), import("@/modules/workspace"),
  import("@/modules/settings"), import("@/modules/live"), import("@/modules/conversation"), import("./fake/event"),
]);

connectTheme();
createRoot(document.getElementById("root")!).render(<App />);

/** Espera a tela assentar: duas pinturas e um respiro. */
const settle = (ms = 250) => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(resolve, ms))));

window.__studio = {
  ready: async () => {
    for (let index = 0; index < 100 && !document.querySelector("[data-tour], main, aside"); index++) await settle(100);
    await settle(400);
  },
  view: async (view: string) => { navigation.navigate(view as never); await settle(); },
  settings: async (tab: string) => { settings.openSettingsTab(tab as never); await settle(400); },
  project: async (projectId: string) => { workspace.openProject(projectId); await settle(400); },
  chat: async (chatId: string) => { workspace.openChat(chatId); await settle(500); },
  live: async (chatId: string, path?: string) => { live.setLivePanel(chatId, true); if (path) live.selectLiveFile(chatId, path); await settle(600); },
  emit: async (event: string, payload: unknown) => { emits.emitCore(event, payload); await settle(); },
  modules: { navigation, workspace, settings, live, conversation },
};

declare global {
  interface Window { __studio: Record<string, unknown> }
}
