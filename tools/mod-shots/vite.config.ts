import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig, type Plugin } from "vite";

// O estúdio dos prints: a tela do app de verdade, no navegador, com o núcleo
// (o Tauri) e o Supabase trocados por dados de exemplo (`fake/`). Só serve
// para gerar os prints das páginas dos mods (`shoot.mjs`); nada daqui entra
// no build do app.
const here = (path: string) => fileURLToPath(new URL(path, import.meta.url));
// O botão da página de cada mod no site só aparece com o endereço do site.
process.env.VITE_SITE_URL ??= "https://jayv.example";
const root = here("../..");

/** O cliente do Supabase do app vira o de mentira, de onde quer que venha o
 * import (`./client` dentro de `modules/auth` ou `@/modules/auth/client`). */
function fakeSupabase(): Plugin {
  const real = `${root}src/modules/auth/client.ts`;
  return {
    name: "studio-fake-supabase",
    enforce: "pre",
    async resolveId(source, importer, options) {
      if (!importer || importer.includes("/tools/mod-shots/")) return null;
      const resolved = await this.resolve(source, importer, { ...options, skipSelf: true });
      return resolved?.id.split("?")[0] === real ? here("./fake/supabase.ts") : null;
    },
  };
}

/** Os dados de exemplo de cada página: o catálogo do repositório de mods
 * (`STUDIO_MODS`, padrão `../jayv-coder-mods` ao lado deste repositório), as
 * traduções que o `shoot.mjs` baixou para o cache (`STUDIO_CACHE`) e os textos
 * das conversas (`samples.<idioma>.json`, sobre o inglês). */
function studioData(): Plugin {
  const mods = process.env.STUDIO_MODS ?? `${root}../jayv-coder-mods`;
  const cache = process.env.STUDIO_CACHE ?? here("./.cache");
  const json = (path: string, fallback: unknown) => (existsSync(path) ? JSON.parse(readFileSync(path, "utf8")) : fallback);
  return {
    name: "studio-data",
    configureServer(server) {
      server.middlewares.use("/@studio/data", (request, response) => {
        const locale = new URL(request.url ?? "", "http://studio").searchParams.get("locale") ?? "en";
        const body = {
          locale,
          catalog: json(`${mods}/catalog.v2.json`, null),
          translations: json(`${cache}/translations/${locale}.json`, {}),
          samples: { ...json(here("./samples.en.json"), {}), ...json(here(`./samples.${locale}.json`), {}) },
          locales: json(`${cache}/locales.json`, [{ id: "en", name: "English", rtl: false, position: 0 }]),
        };
        response.setHeader("Content-Type", "application/json");
        response.end(JSON.stringify(body));
      });
    },
  };
}

export default defineConfig({
  root,
  plugins: [studioData(), fakeSupabase(), react(), tailwindcss()],
  resolve: {
    alias: [
      { find: "@tauri-apps/api/core", replacement: here("./fake/core.ts") },
      { find: "@tauri-apps/api/event", replacement: here("./fake/event.ts") },
      { find: "@tauri-apps/api/app", replacement: here("./fake/app.ts") },
      { find: /^@tauri-apps\/plugin-.*$/, replacement: here("./fake/plugin.ts") },
      { find: "@", replacement: `${root}src` },
    ],
  },
  clearScreen: false,
  server: { port: 5199, strictPort: true, host: "127.0.0.1" },
});
