#!/usr/bin/env node
// Os prints das páginas dos mods no site, tirados da tela do app de verdade
// com os dados de exemplo do estúdio (`fake/`), em cada idioma do app. Cada
// print vai para `mods/<id>/media/<nome>.<idioma>.webp` no repositório de mods,
// onde a página do mod (`page.json`) o cita.
//
//   node tools/mod-shots/shoot.mjs [--mods ../jayv-coder-mods] [--locales en,pt-BR] [--only rea,stats] [--scenes enable] [--theme light]
//
// Precisa do Chromium do Playwright (`playwright-core`, já nas
// devDependencies) e de um Chrome ou Chromium na máquina: `--chrome <caminho>`
// ou `CHROME_PATH`. As traduções saem do Supabase público do app; o texto que
// ainda não foi publicado vem de `--overlay <pasta>` (um `<idioma>.json` por
// idioma), para o print novo já sair traduzido.
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");

function option(name, fallback) {
  const at = process.argv.indexOf(`--${name}`);
  return at >= 0 ? process.argv[at + 1] : fallback;
}

const MODS = resolve(option("mods", process.env.STUDIO_MODS ?? join(root, "../jayv-coder-mods")));
const CACHE = resolve(option("cache", join(here, ".cache")));
const THEME = option("theme", "light");
const OVERLAY = option("overlay", null);
const ONLY = option("only", "").split(",").filter(Boolean);
/** Só estes prints de cada mod (`enable`, `chat`…): o que a tela mudou. */
const ONLY_SCENES = option("scenes", "").split(",").filter(Boolean);
const PORT = 5199;
const SUPABASE = "https://exvsozyemolrjbjetqww.supabase.co";
const KEY = "sb_publishable_7PeIEX0yh_svIEeNw3Gwew_lEbMVtMm";
const VIEWPORT = { width: 1280, height: 800 };
const QUALITY = 0.82;

/** O Playwright: o do projeto ou o que a máquina já tiver. */
async function playwright() {
  for (const name of ["playwright-core", "playwright", "/opt/npm-tools/node_modules/playwright/index.mjs"]) {
    try { return await import(name); } catch { /* o próximo */ }
  }
  throw new Error("playwright-core not found: run npm install");
}

/** As traduções de um idioma, todas as chaves, página por página. */
async function translations(locale) {
  const out = {};
  for (let page = 0; page < 40; page++) {
    const response = await fetch(`${SUPABASE}/rest/v1/translations?select=key,value&locale=eq.${encodeURIComponent(locale)}&order=key.asc&limit=1000&offset=${page * 1000}`, { headers: { apikey: KEY } });
    if (!response.ok) throw new Error(`translations ${locale}: ${response.status}`);
    const rows = await response.json();
    if (!rows.length) break;
    for (const row of rows) out[row.key] = row.value;
  }
  return out;
}

async function prepare(locales) {
  mkdirSync(join(CACHE, "translations"), { recursive: true });
  const response = await fetch(`${SUPABASE}/rest/v1/locales?select=id,name,rtl,position&order=position.asc`, { headers: { apikey: KEY } });
  const list = await response.json();
  writeFileSync(join(CACHE, "locales.json"), JSON.stringify(list));
  for (const locale of locales) {
    const texts = locale === "en" ? {} : await translations(locale);
    const extra = OVERLAY && existsSync(join(OVERLAY, `${locale}.json`)) ? JSON.parse(readFileSync(join(OVERLAY, `${locale}.json`), "utf8")) : {};
    writeFileSync(join(CACHE, "translations", `${locale}.json`), JSON.stringify({ ...texts, ...extra }));
  }
  return list.map((row) => row.id);
}

async function server() {
  const child = spawn(process.execPath, [join(root, "node_modules/vite/bin/vite.js"), "--config", join(here, "vite.config.ts")], {
    cwd: root, env: { ...process.env, STUDIO_MODS: MODS, STUDIO_CACHE: CACHE }, stdio: ["ignore", "pipe", "pipe"],
  });
  await new Promise((done, fail) => {
    const timer = setTimeout(() => fail(new Error("vite did not start")), 30_000);
    child.stdout.on("data", (chunk) => { if (String(chunk).includes("ready")) { clearTimeout(timer); done(); } });
    child.on("exit", (code) => fail(new Error(`vite exited ${code}`)));
  });
  return child;
}

const titleOf = (catalog, id, locale) => {
  const title = catalog.mods.find((mod) => mod.id === id)?.title ?? {};
  return title[locale] ?? title.en ?? id;
};

/** Rola até o elemento que tem o texto (no idioma da página) e o centraliza. */
async function reveal(page, selector) {
  await page.locator(selector).first().scrollIntoViewIfNeeded();
  await page.evaluate((target) => document.querySelector(target)?.scrollIntoView({ block: "center" }), selector);
  await page.waitForTimeout(300);
}

/** Rola até o controle cujo rótulo é a chave do i18n dada, no idioma da página. */
async function revealText(page, key, block = "center") {
  const text = await page.evaluate((name) => window.__studioData.translations[name] ?? null, key);
  const english = await page.evaluate(async (name) => (await import("/src/modules/i18n/messages/en.ts")).en[name], key);
  const label = typeof text === "string" ? text : english;
  const found = page.getByText(label, { exact: false }).first();
  await found.evaluate((element, where) => element.scrollIntoView({ block: where }), block);
  await page.waitForTimeout(300);
}

const studio = (page, name, ...args) => page.evaluate(([method, values]) => window.__studio[method](...values), [name, args]);

/** As cenas de cada mod: o print `enable` (a aba Mods com o mod) e os de uso,
 * pelo nome que o `page.json` cita. */
const AGENT_TAB = { claude: "claude", codex: "codex", copilot: "copilot", cursor: "cursor", kilo: "kilo", openrouter: "openrouter", litellm: "litellm", ollama: "mod-ollama", "lm-studio": "mod-lm-studio" };
const AGENT_CHAT = { claude: "c-claude", codex: "c-codex", copilot: "c-copilot", cursor: "c-cursor", kilo: "c-kilo", openrouter: "c-openrouter", litellm: "c-litellm", ollama: "c-ollama", "lm-studio": "c-lmstudio" };

const SCENES = {
  ...Object.fromEntries(Object.keys(AGENT_TAB).map((id) => [id, {
    settings: async (page) => { await studio(page, "settings", AGENT_TAB[id]); },
    chat: async (page) => { await studio(page, "chat", AGENT_CHAT[id]); },
  }])),
  stats: { stats: async (page) => { await studio(page, "view", "stats"); } },
  gateBoard: { gate: async (page) => { await studio(page, "project", "p1"); await studio(page, "view", "gate"); } },
  liveFiles: {
    live: async (page) => {
      await studio(page, "chat", "c-live");
      // O painel de andamento recolhido deixa a conversa e o painel Código lado a lado.
      await page.locator('aside button[aria-expanded="true"]').first().click();
      await studio(page, "live", "c-live", "src/api/invoices.rs");
    },
    repos: async (page) => {
      await studio(page, "chat", "c-live");
      await page.locator('aside button[aria-expanded="true"]').first().click();
      await studio(page, "live", "c-live");
      await page.locator('[data-tour="live-repos"]').click();
      await page.waitForTimeout(600);
    },
  },
  chatSearch: {
    search: async (page) => {
      await studio(page, "project", "p1");
      await studio(page, "view", "chats");
      const query = await page.evaluate(() => window.__studioData.samples["search.query"]);
      await page.locator('input[type="search"]').first().fill(query);
      await page.waitForTimeout(900);
    },
  },
  projectNotes: {
    memory: async (page) => {
      await studio(page, "project", "p1");
      await studio(page, "view", "chats");
      await page.locator("button:has(svg.lucide-brain)").first().click();
      await page.waitForTimeout(700);
    },
  },
  secondOpinion: {
    settings: async (page) => { await studio(page, "settings", "jev"); await reveal(page, "#jev-review"); },
    chat: async (page) => { await studio(page, "chat", "c-review"); },
  },
  planFirst: {
    settings: async (page) => { await studio(page, "settings", "jev"); await reveal(page, "#jev-plan-first"); },
    chat: async (page) => { await studio(page, "chat", "c-plan"); },
  },
  parallelTasks: {
    settings: async (page) => { await studio(page, "settings", "jev"); await reveal(page, "#jev-parallel"); },
    chat: async (page) => { await studio(page, "chat", "c-split"); },
  },
  answerRecall: { stats: async (page) => { await studio(page, "view", "stats"); await revealText(page, "usage.jev.title", "start"); } },
  leanCode: { profile: async (page) => { await studio(page, "view", "profile"); await reveal(page, "#expertise-lean"); } },
  symbolIndex: { settings: async (page) => { await studio(page, "settings", "claude"); await reveal(page, "#claude-symbols"); } },
  mcp: {
    settings: async (page) => { await studio(page, "settings", "mcp"); },
    agent: async (page) => { await studio(page, "settings", "claude"); await reveal(page, "#claude-approve-mcps"); },
  },
  skills: {
    settings: async (page) => { await studio(page, "settings", "skills"); },
    chat: async (page) => { await studio(page, "chat", "c-skill"); },
  },
  skillsHub: {
    settings: async (page) => {
      await studio(page, "settings", "skills");
      const box = page.locator('form:has(button[type="submit"]) input').first();
      await box.fill("release notes");
      await page.locator('form:has(button[type="submit"]) button[type="submit"]').first().click();
      await page.waitForTimeout(700);
      await box.evaluate((element) => (element.closest("[data-slot=card]") ?? element).scrollIntoView({ block: "start" }));
      await page.waitForTimeout(300);
    },
  },
  conversationFind: {
    chat: async (page) => {
      await studio(page, "chat", "c-find");
      const query = await page.evaluate(() => window.__studioData.samples["find.query"]);
      await page.evaluate(async (text) => { const conversation = await import("/src/modules/conversation/index.ts"); conversation.openChatFind(); conversation.setFindQuery(text); }, query);
      await page.waitForTimeout(700);
    },
  },
  rea: {
    settings: async (page) => { await studio(page, "settings", "claude"); await reveal(page, "#claude-approve-mcps"); },
    chat: async (page) => { await studio(page, "chat", "c-rea"); },
  },
};

/** O print `enable`: a aba Mods, filtrada pelo nome do mod, com a linha dele
 * em destaque (a busca também acha outros mods com a mesma palavra), como fica
 * depois de instalar pela página dele no site: na aba Instalados, ligado; o
 * essencial, na aba Essenciais. */
async function enable(page, catalog, id, locale) {
  const title = titleOf(catalog, id, locale);
  await studio(page, "settings", "mods");
  if (catalog.mods.find((mod) => mod.id === id)?.required) await page.locator('[data-tour="settings-mods-tab-essentials"]').click();
  await page.locator('[data-tour="settings-mods-search"] input').fill(title);
  await page.waitForTimeout(400);
  await page.locator('[data-tour="settings-mods-search"]').evaluate((element) => element.scrollIntoView({ block: "start" }));
  await page.evaluate((name) => {
    const row = [...document.querySelectorAll("li")].find((item) => item.querySelector("span.truncate")?.textContent?.trim() === name);
    if (row) Object.assign(row.style, { outline: "2px solid var(--primary)", outlineOffset: "3px" });
  }, title);
  await page.waitForTimeout(300);
}

/** O PNG da tela vira WebP pelo próprio Chromium (o `canvas` sabe codificar). */
async function webp(page, png) {
  const data = await page.evaluate(async ([base64, quality]) => {
    const image = new Image();
    image.src = `data:image/png;base64,${base64}`;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = image.width;
    canvas.height = image.height;
    canvas.getContext("2d").drawImage(image, 0, 0);
    return canvas.toDataURL("image/webp", quality).split(",")[1];
  }, [png.toString("base64"), QUALITY]);
  return Buffer.from(data, "base64");
}

async function main() {
  const catalog = JSON.parse(readFileSync(join(MODS, "catalog.v2.json"), "utf8"));
  const wanted = option("locales", null);
  const locales = await prepare(wanted ? wanted.split(",") : ["en", "pt-BR", "es", "zh-CN", "hi", "ar", "fr", "ru", "ja", "de"]).then((all) => (wanted ? wanted.split(",") : all));
  const vite = await server();
  const { chromium } = await playwright();
  const browser = await chromium.launch({ executablePath: option("chrome", process.env.CHROME_PATH) || undefined });
  const ids = catalog.mods.map((mod) => mod.id).filter((id) => !ONLY.length || ONLY.includes(id));
  let count = 0;
  try {
    for (const locale of locales) {
      for (const id of ids) {
        const scenes = { enable: (page) => enable(page, catalog, id, locale), ...(SCENES[id] ?? {}) };
        for (const [name, scene] of Object.entries(scenes).filter(([name]) => !ONLY_SCENES.length || ONLY_SCENES.includes(name))) {
          // Uma recarga do vite no meio da cena (dependência otimizada de
          // novo, por exemplo) derruba o contexto da página: a cena roda de
          // novo, numa página nova, antes de desistir do estúdio inteiro.
          for (let attempt = 1; ; attempt++) {
            const context = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 1, timezoneId: "UTC", locale: locale === "en" ? "en-US" : locale });
            try {
              const page = await context.newPage();
              const errors = [];
              page.on("pageerror", (error) => errors.push(error.message));
              await page.clock.setFixedTime(new Date("2026-10-13T14:30:00Z"));
              await page.goto(`http://127.0.0.1:${PORT}/tools/mod-shots/index.html?locale=${encodeURIComponent(locale)}&theme=${THEME}`, { waitUntil: "networkidle" });
              await page.waitForFunction(() => window.__studio, null, { timeout: 30_000 });
              await studio(page, "ready");
              await scene(page);
              await page.waitForTimeout(300);
              const out = join(MODS, "mods", id, "media", `${name}.${locale}.webp`);
              mkdirSync(dirname(out), { recursive: true });
              writeFileSync(out, await webp(page, await page.screenshot()));
              count++;
              if (errors.length) console.error(`! ${id}/${name}.${locale}: ${errors.join(" | ")}`);
              break;
            } catch (error) {
              if (attempt >= 3) throw error;
              console.error(`~ ${id}/${name}.${locale}: attempt ${attempt} failed (${String(error?.message ?? error).split("\n")[0]})`);
            } finally {
              await context.close();
            }
          }
        }
      }
      console.log(`${locale}: done`);
    }
  } finally {
    await browser.close();
    vite.kill();
  }
  console.log(`${count} prints`);
}

await main();
